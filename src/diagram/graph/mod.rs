use std::collections::{HashMap, VecDeque};

use super::canvas::NodeShape;
use flowchart::{Edge, Graph, Node};

pub(crate) mod class;
pub(crate) mod er;
pub(crate) mod flowchart;
pub(crate) mod mindmap;
pub(crate) mod state;

#[derive(Clone)]
pub(crate) struct NodeLayout {
    pub(crate) center_x: usize,
    pub(crate) top_y: usize,
    pub(crate) width: usize,
    pub(crate) height: usize,
}

fn assign_layers(graph: &Graph) -> Vec<Vec<String>> {
    // Build adjacency and in-degree
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();

    for id in graph.nodes.keys() {
        in_degree.entry(id.as_str()).or_insert(0);
        adj.entry(id.as_str()).or_default();
    }

    for edge in &graph.edges {
        adj.entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
        *in_degree.entry(edge.to.as_str()).or_insert(0) += 1;
    }

    // Kahn's topological sort
    let mut queue: VecDeque<&str> = VecDeque::new();
    let mut topo_order: Vec<String> = Vec::new();
    let mut in_deg = in_degree.clone();

    for id in &graph.node_order {
        if in_deg.get(id.as_str()).copied().unwrap_or(0) == 0 {
            queue.push_back(id.as_str());
        }
    }

    // Cycle fallback
    if queue.is_empty()
        && let Some(first) = graph.node_order.first()
    {
        queue.push_back(first.as_str());
    }

    while let Some(node) = queue.pop_front() {
        topo_order.push(node.to_string());
        if let Some(neighbors) = adj.get(node) {
            for &next in neighbors {
                let deg = in_deg.get_mut(next).unwrap();
                *deg = deg.saturating_sub(1);
                if *deg == 0 {
                    queue.push_back(next);
                }
            }
        }
    }

    // Add any remaining nodes (from cycles)
    for id in &graph.node_order {
        if !topo_order.contains(id) {
            topo_order.push(id.clone());
        }
    }

    // Longest-path layer assignment
    let mut node_layer: HashMap<String, usize> = HashMap::new();
    for node in &topo_order {
        let mut max_parent_layer: Option<usize> = None;
        for edge in &graph.edges {
            if edge.to == *node
                && let Some(&parent_layer) = node_layer.get(&edge.from)
            {
                max_parent_layer =
                    Some(max_parent_layer.map_or(parent_layer, |m: usize| m.max(parent_layer)));
            }
        }
        let layer = max_parent_layer.map_or(0, |m| m + 1);
        node_layer.insert(node.clone(), layer);
    }

    let max_layer = node_layer.values().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<String>> = vec![Vec::new(); max_layer + 1];
    for node in &topo_order {
        let layer = node_layer[node];
        layers[layer].push(node.clone());
    }
    layers.retain(|l| !l.is_empty());
    layers
}

fn order_within_layers(layers: &mut [Vec<String>], graph: &Graph) {
    // Barycenter heuristic to reduce edge crossings
    for _ in 0..4 {
        // Forward pass
        for i in 1..layers.len() {
            let prev_layer = layers[i - 1].clone();
            let mut positions: Vec<(String, f64)> = Vec::new();

            for node in &layers[i] {
                let mut parent_positions: Vec<f64> = Vec::new();
                for edge in &graph.edges {
                    if edge.to == *node
                        && let Some(pos) = prev_layer.iter().position(|n| n == &edge.from)
                    {
                        parent_positions.push(pos as f64);
                    }
                }
                let avg = if parent_positions.is_empty() {
                    layers[i].iter().position(|n| n == node).unwrap_or(0) as f64
                } else {
                    parent_positions.iter().sum::<f64>() / parent_positions.len() as f64
                };
                positions.push((node.clone(), avg));
            }
            positions.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            layers[i] = positions.into_iter().map(|(n, _)| n).collect();
        }

        // Backward pass
        for i in (0..layers.len().saturating_sub(1)).rev() {
            let next_layer = layers[i + 1].clone();
            let mut positions: Vec<(String, f64)> = Vec::new();

            for node in &layers[i] {
                let mut child_positions: Vec<f64> = Vec::new();
                for edge in &graph.edges {
                    if edge.from == *node
                        && let Some(pos) = next_layer.iter().position(|n| n == &edge.to)
                    {
                        child_positions.push(pos as f64);
                    }
                }
                let avg = if child_positions.is_empty() {
                    layers[i].iter().position(|n| n == node).unwrap_or(0) as f64
                } else {
                    child_positions.iter().sum::<f64>() / child_positions.len() as f64
                };
                positions.push((node.clone(), avg));
            }
            positions.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            layers[i] = positions.into_iter().map(|(n, _)| n).collect();
        }
    }
}

fn adjacent_layer_crossing_score(
    layers: &[Vec<String>],
    graph: &Graph,
    left_layer: usize,
) -> usize {
    let Some(left) = layers.get(left_layer) else {
        return 0;
    };
    let Some(right) = layers.get(left_layer + 1) else {
        return 0;
    };

    let left_positions: HashMap<&str, usize> = left
        .iter()
        .enumerate()
        .map(|(idx, id)| (id.as_str(), idx))
        .collect();
    let right_positions: HashMap<&str, usize> = right
        .iter()
        .enumerate()
        .map(|(idx, id)| (id.as_str(), idx))
        .collect();

    let mut layer_edges: Vec<(usize, usize)> = graph
        .edges
        .iter()
        .filter_map(|edge| {
            Some((
                *left_positions.get(edge.from.as_str())?,
                *right_positions.get(edge.to.as_str())?,
            ))
        })
        .collect();

    layer_edges.sort_unstable_by_key(|&(from, to)| (from, to));

    let mut crossings = 0;
    for i in 0..layer_edges.len() {
        for j in (i + 1)..layer_edges.len() {
            if layer_edges[i].0 < layer_edges[j].0 && layer_edges[i].1 > layer_edges[j].1 {
                crossings += 1;
            }
        }
    }
    crossings
}

fn total_lr_crossing_score(layers: &[Vec<String>], graph: &Graph) -> usize {
    (0..layers.len().saturating_sub(1))
        .map(|idx| adjacent_layer_crossing_score(layers, graph, idx))
        .sum()
}

fn refine_lr_layer_order(layers: &mut [Vec<String>], graph: &Graph) {
    if layers.len() < 2 {
        return;
    }

    for _ in 0..4 {
        let mut improved = false;

        for layer_idx in 0..layers.len() {
            if layers[layer_idx].len() < 2 {
                continue;
            }

            let mut pos = 0;
            while pos + 1 < layers[layer_idx].len() {
                let before = total_lr_crossing_score(layers, graph);
                layers[layer_idx].swap(pos, pos + 1);
                let after = total_lr_crossing_score(layers, graph);

                if after < before {
                    improved = true;
                    pos += 1;
                } else {
                    layers[layer_idx].swap(pos, pos + 1);
                }

                pos += 1;
            }
        }

        if !improved {
            break;
        }
    }
}

fn lr_node_extra_gap(graph: &Graph, node_id: &str) -> usize {
    let degree = graph
        .edges
        .iter()
        .filter(|edge| edge.from == node_id || edge.to == node_id)
        .count();

    match degree {
        0 | 1 => 0,
        2 | 3 => 2,
        4 | 5 => 3,
        _ => 4,
    }
}

fn lr_node_port_count(graph: &Graph, node_id: &str) -> usize {
    let outgoing = graph
        .edges
        .iter()
        .filter(|edge| edge.from == node_id)
        .count();
    let incoming = graph.edges.iter().filter(|edge| edge.to == node_id).count();
    outgoing.max(incoming).max(1)
}

fn lr_node_height(graph: &Graph, node_id: &str) -> usize {
    let port_count = lr_node_port_count(graph, node_id);
    if port_count <= 1 {
        3
    } else {
        port_count * 3 + 1
    }
}

fn lr_layer_height(
    layer: &[String],
    graph: &Graph,
    node_heights: &HashMap<String, usize>,
    base_gap: usize,
) -> usize {
    if layer.is_empty() {
        return 0;
    }

    let nodes_height = layer
        .iter()
        .map(|id| {
            node_heights
                .get(id)
                .copied()
                .unwrap_or_else(|| lr_node_height(graph, id))
        })
        .sum::<usize>();
    let gaps_height = layer
        .windows(2)
        .map(|pair| {
            let left_gap = lr_node_extra_gap(graph, &pair[0]);
            let right_gap = lr_node_extra_gap(graph, &pair[1]);
            base_gap + left_gap.max(right_gap)
        })
        .sum::<usize>();

    nodes_height + gaps_height
}

fn node_box_width(node: &Node) -> usize {
    label_box_width(&node.label, node.shape)
}

pub(crate) fn label_box_width(label: &str, shape: NodeShape) -> usize {
    let label_width = label.chars().count();
    let width = match shape {
        NodeShape::Diamond => label_width + 6,
        _ => label_width + 4,
    };
    width.max(7)
}

fn node_left_x(layout: &NodeLayout) -> usize {
    layout.center_x.saturating_sub(layout.width / 2)
}

fn node_right_x(layout: &NodeLayout) -> usize {
    node_left_x(layout) + layout.width.saturating_sub(1)
}

fn lr_lane_key(edge: &Edge, layer_by_id: &HashMap<String, usize>) -> Option<(usize, usize)> {
    Some((*layer_by_id.get(&edge.from)?, *layer_by_id.get(&edge.to)?))
}

fn lr_lane_counts(
    graph: &Graph,
    positions: &HashMap<String, NodeLayout>,
    layer_by_id: &HashMap<String, usize>,
    outgoing_ports: &[(usize, usize)],
    incoming_ports: &[(usize, usize)],
) -> HashMap<(usize, usize), usize> {
    let mut counts = HashMap::new();
    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        if let (Some(src), Some(dst), Some(key)) = (
            positions.get(&edge.from),
            positions.get(&edge.to),
            lr_lane_key(edge, layer_by_id),
        ) {
            let (src_port_idx, src_port_count) = outgoing_ports[edge_idx];
            let (dst_port_idx, dst_port_count) = incoming_ports[edge_idx];
            let src_cy = lr_edge_port_y(src.top_y, src.height, src_port_idx, src_port_count);
            let dst_cy = lr_edge_port_y(dst.top_y, dst.height, dst_port_idx, dst_port_count);
            if src_cy != dst_cy {
                *counts.entry(key).or_insert(0) += 1;
            }
        }
    }
    counts
}

fn lr_lane_mid_x(
    src_right_x: usize,
    dst_left_x: usize,
    lane_idx: usize,
    lane_count: usize,
) -> Option<usize> {
    if lane_count <= 1 || src_right_x + 1 >= dst_left_x {
        return None;
    }

    let first = src_right_x + 1;
    let slots = dst_left_x.saturating_sub(first);
    if slots == 0 {
        return None;
    }

    let lane = lane_idx.min(lane_count - 1);
    Some(first + ((lane + 1) * slots / (lane_count + 1)))
}

fn lr_edge_port_y(top_y: usize, height: usize, port_idx: usize, port_count: usize) -> usize {
    if port_count <= 1 {
        return top_y + height / 2;
    }

    let content_rows = height.saturating_sub(2).max(1);
    if content_rows <= 1 {
        return top_y + 1;
    }

    let slot = port_idx.min(port_count - 1) * (content_rows - 1) / (port_count - 1);
    top_y + 1 + slot
}

#[allow(clippy::type_complexity)]
fn lr_edge_port_maps(
    graph: &Graph,
    positions: &HashMap<String, NodeLayout>,
) -> (Vec<(usize, usize)>, Vec<(usize, usize)>) {
    let mut outgoing_ports = vec![(0, 1); graph.edges.len()];
    let mut incoming_ports = vec![(0, 1); graph.edges.len()];

    let mut outgoing_by_node: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut incoming_by_node: HashMap<&str, Vec<usize>> = HashMap::new();
    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        outgoing_by_node
            .entry(edge.from.as_str())
            .or_default()
            .push(edge_idx);
        incoming_by_node
            .entry(edge.to.as_str())
            .or_default()
            .push(edge_idx);
    }

    for edge_indices in outgoing_by_node.values_mut() {
        edge_indices.sort_by_key(|&edge_idx| {
            let edge = &graph.edges[edge_idx];
            let dst_y = positions
                .get(&edge.to)
                .map(|layout| layout.top_y + layout.height / 2)
                .unwrap_or(usize::MAX);
            (dst_y, edge_idx)
        });
        let port_count = edge_indices.len();
        for (port_idx, &edge_idx) in edge_indices.iter().enumerate() {
            outgoing_ports[edge_idx] = (port_idx, port_count);
        }
    }

    for edge_indices in incoming_by_node.values_mut() {
        edge_indices.sort_by_key(|&edge_idx| {
            let edge = &graph.edges[edge_idx];
            let src_y = positions
                .get(&edge.from)
                .map(|layout| layout.top_y + layout.height / 2)
                .unwrap_or(usize::MAX);
            (src_y, edge_idx)
        });
        let port_count = edge_indices.len();
        for (port_idx, &edge_idx) in edge_indices.iter().enumerate() {
            incoming_ports[edge_idx] = (port_idx, port_count);
        }
    }

    (outgoing_ports, incoming_ports)
}
