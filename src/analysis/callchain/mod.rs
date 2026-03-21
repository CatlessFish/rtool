use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use rustc_middle::ty::TyCtxt;

use crate::analysis::callgraph::default::{CallGraphAnalyzer, CallGraphInfo};
use crate::{rtool_error, rtool_info, rtool_warn};

pub struct CallChainAnalyzer<'tcx> {
    tcx: TyCtxt<'tcx>,
    from_query: String,
    to_query: String,
    all_paths: bool,
    output_file: Option<String>,
}

impl<'tcx> CallChainAnalyzer<'tcx> {
    pub fn new(
        tcx: TyCtxt<'tcx>,
        from_query: &str,
        to_query: &str,
        all_paths: bool,
        output_file: Option<String>,
    ) -> Self {
        Self {
            tcx,
            from_query: from_query.to_string(),
            to_query: to_query.to_string(),
            all_paths,
            output_file,
        }
    }

    pub fn start(&mut self) {
        rtool_info!("Executing callchain analysis");
        let mut callgraph_analyzer = CallGraphAnalyzer::new(self.tcx);
        callgraph_analyzer.start();

        let graph = &callgraph_analyzer.graph;
        let from_nodes = self.resolve_candidates(graph, &self.from_query);
        let to_nodes = self.resolve_candidates(graph, &self.to_query);

        if from_nodes.is_empty() {
            self.emit_no_match("from", &self.from_query);
            return;
        }
        if to_nodes.is_empty() {
            self.emit_no_match("to", &self.to_query);
            return;
        }

        let rendered_paths = if self.all_paths {
            self.collect_all_paths(graph, &from_nodes, &to_nodes)
        } else {
            self.find_first_path(graph, &from_nodes, &to_nodes)
                .into_iter()
                .collect()
        };

        let exists = !rendered_paths.is_empty();
        rtool_info!(
            "Callchain exists from '{}' to '{}': {}",
            self.from_query,
            self.to_query,
            if exists { "yes" } else { "no" }
        );

        if self.all_paths || self.output_file.is_some() {
            self.write_path_output(exists, &rendered_paths);
        }
    }

    fn resolve_candidates(&self, graph: &CallGraphInfo<'tcx>, query: &str) -> Vec<usize> {
        let mut node_ids = graph
            .functions
            .iter()
            .filter_map(|(node_id, node)| {
                let def_path = node.get_def_path();
                let def_id_str = format!("{:?}", node.get_def_id());
                let last_segment = def_path.split("::").last().unwrap_or("");

                if def_path == query || def_id_str.contains(query) || last_segment.contains(query) {
                    Some(*node_id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        node_ids.sort_by_key(|node_id| self.node_path(graph, *node_id));
        node_ids.dedup();
        node_ids
    }

    fn find_first_path(
        &self,
        graph: &CallGraphInfo<'tcx>,
        from_nodes: &[usize],
        to_nodes: &[usize],
    ) -> Option<String> {
        let target_set = to_nodes.iter().copied().collect::<HashSet<_>>();
        let mut sorted_sources = from_nodes.to_vec();
        sorted_sources.sort_by_key(|node_id| self.node_path(graph, *node_id));

        for source in sorted_sources {
            let mut path = vec![source];
            let mut visited = HashSet::from([source]);
            if self.find_first_path_from(graph, source, &target_set, &mut visited, &mut path) {
                return Some(self.render_path(graph, &path));
            }
        }

        None
    }

    fn find_first_path_from(
        &self,
        graph: &CallGraphInfo<'tcx>,
        current: usize,
        targets: &HashSet<usize>,
        visited: &mut HashSet<usize>,
        path: &mut Vec<usize>,
    ) -> bool {
        if targets.contains(&current) {
            return true;
        }

        for callee in self.sorted_callees(graph, current) {
            if !visited.insert(callee) {
                continue;
            }

            path.push(callee);
            if self.find_first_path_from(graph, callee, targets, visited, path) {
                return true;
            }
            path.pop();
            visited.remove(&callee);
        }

        false
    }

    fn collect_all_paths(
        &self,
        graph: &CallGraphInfo<'tcx>,
        from_nodes: &[usize],
        to_nodes: &[usize],
    ) -> Vec<String> {
        let target_set = to_nodes.iter().copied().collect::<HashSet<_>>();
        let mut seen_paths = HashSet::<Vec<usize>>::new();
        let mut rendered_paths = Vec::new();
        let mut sorted_sources = from_nodes.to_vec();
        sorted_sources.sort_by_key(|node_id| self.node_path(graph, *node_id));

        for source in sorted_sources {
            let mut path = vec![source];
            let mut visited = HashSet::from([source]);
            self.collect_all_paths_from(
                graph,
                source,
                &target_set,
                &mut visited,
                &mut path,
                &mut seen_paths,
                &mut rendered_paths,
            );
        }

        rendered_paths
    }

    fn collect_all_paths_from(
        &self,
        graph: &CallGraphInfo<'tcx>,
        current: usize,
        targets: &HashSet<usize>,
        visited: &mut HashSet<usize>,
        path: &mut Vec<usize>,
        seen_paths: &mut HashSet<Vec<usize>>,
        rendered_paths: &mut Vec<String>,
    ) {
        if targets.contains(&current) && seen_paths.insert(path.clone()) {
            rendered_paths.push(self.render_path(graph, path));
        }

        for callee in self.sorted_callees(graph, current) {
            if !visited.insert(callee) {
                continue;
            }

            path.push(callee);
            self.collect_all_paths_from(
                graph,
                callee,
                targets,
                visited,
                path,
                seen_paths,
                rendered_paths,
            );
            path.pop();
            visited.remove(&callee);
        }
    }

    fn sorted_callees(&self, graph: &CallGraphInfo<'tcx>, caller: usize) -> Vec<usize> {
        let mut unique_callees = HashSet::new();
        let mut callees = graph
            .fn_calls
            .get(&caller)
            .into_iter()
            .flat_map(|edges| edges.iter())
            .filter_map(|(callee, _)| unique_callees.insert(*callee).then_some(*callee))
            .collect::<Vec<_>>();
        callees.sort_by_key(|node_id| self.node_path(graph, *node_id));
        callees
    }

    fn render_path(&self, graph: &CallGraphInfo<'tcx>, path: &[usize]) -> String {
        path.iter()
            .map(|node_id| self.node_path(graph, *node_id))
            .collect::<Vec<_>>()
            .join("\n-> ")
    }

    fn node_path(&self, graph: &CallGraphInfo<'tcx>, node_id: usize) -> String {
        graph
            .functions
            .get(&node_id)
            .map(|node| node.get_def_path())
            .unwrap_or_else(|| format!("<missing-node:{}>", node_id))
    }

    fn emit_no_match(&self, side: &str, query: &str) {
        rtool_warn!("No function matched {} query '{}'", side, query);
        if self.output_file.is_some() {
            self.write_lines(&[format!("No function matched {} query '{}'", side, query)]);
        }
    }

    fn write_path_output(&self, exists: bool, rendered_paths: &[String]) {
        let mut lines = vec![format!(
            "Callchain exists from '{}' to '{}': {}",
            self.from_query,
            self.to_query,
            if exists { "yes" } else { "no" }
        )];

        if exists {
            for (idx, path) in rendered_paths.iter().enumerate() {
                lines.push(format!("Path {}:", idx + 1));
                lines.push(path.clone());
            }
        } else {
            lines.push("No callchain found.".to_string());
        }

        self.write_lines(&lines);
    }

    fn write_lines(&self, lines: &[String]) {
        let mut writer = match self.make_writer() {
            Some(writer) => writer,
            None => return,
        };

        for line in lines {
            if let Err(err) = writer.write_all(line.as_bytes()) {
                rtool_error!("Failed to write callchain output: {}", err);
                return;
            }
            if let Err(err) = writer.write_all(b"\n") {
                rtool_error!("Failed to write callchain output: {}", err);
                return;
            }
        }

        if let Err(err) = writer.flush() {
            rtool_error!("Failed to flush callchain output: {}", err);
        }
    }

    fn make_writer(&self) -> Option<Box<dyn Write>> {
        match &self.output_file {
            Some(path) => {
                let os_path = Path::new(path);
                match File::create(os_path) {
                    Ok(file) => Some(Box::new(file)),
                    Err(err) => {
                        rtool_error!("Failed to create callchain output file '{}': {}", path, err);
                        None
                    }
                }
            }
            None if self.all_paths => Some(Box::new(io::stdout())),
            None => None,
        }
    }
}
