// use std::{
//     hash::Hash,
//     iter::{from_fn, FromIterator},
// };

// use petgraph::{
//     visit::{
//         NodeCount, IntoNeighborsDirected
//     },
//     Direction::Outgoing
// };

// use indexmap::IndexSet;
// pub fn all_simple_paths<TargetColl, G>(
//     graph: G,
//     from: G::NodeId,
//     to: G::NodeId,
//     min_intermediate_nodes: usize,
//     max_intermediate_nodes: Option<usize>,
// ) -> impl Iterator<Item = TargetColl>
// where
//     G: NodeCount,
//     G: IntoNeighborsDirected,
//     G::NodeId: Eq + Hash,
//     TargetColl: FromIterator<G::NodeId>,
// {
//     // how many nodes are allowed in simple path up to target node
//     // it is min/max allowed path length minus one, because it is more appropriate when implementing lookahead
//     // than constantly add 1 to length of current path
//     let max_length = if let Some(l) = max_intermediate_nodes {
//         l + 1
//     } else {
//         graph.node_count() - 1
//     };

//     let min_length = min_intermediate_nodes + 1;

//     // list of visited nodes
//     let mut visited: IndexSet<G::NodeId> = IndexSet::from_iter(Some(from));
//     // list of childs of currently exploring path nodes,
//     // last elem is list of childs of last visited node
//     let mut stack = vec![graph.neighbors_directed(from, Outgoing)];

//     from_fn(move || {
//         while let Some(children) = stack.last_mut() {
//             if let Some(child) = children.next() {
//                 if visited.len() < max_length {
//                     if child == to {
//                         // Returns path if the node is equivalent to the target
//                         // And the length is shorted than the 
//                         if visited.len() >= min_length {
//                             let path = visited
//                                 .iter()
//                                 .cloned()
//                                 .chain(Some(to))
//                                 .collect::<TargetColl>();
//                             return Some(path);
//                         }
//                     } else if !visited.contains(&child) {
//                         // Adds node to path if it hasn't been visited.
//                         visited.insert(child);
//                         stack.push(graph.neighbors_directed(child, Outgoing));
//                     }
//                 } else {
//                     if (child == to || children.any(|v| v == to)) && visited.len() >= min_length {
//                         let path = visited
//                             .iter()
//                             .cloned()
//                             .chain(Some(to))
//                             .collect::<TargetColl>();
//                         return Some(path);
//                     }
//                     stack.pop();
//                     visited.pop();
//                 }
//             } else {
//                 stack.pop();
//                 visited.pop();
//             }
//         }
//         None
//     })
// }
