//! Draws a pattern graph (positions come from `pattern_graph::layout`).

use crate::theme::{accent, bad, dim, hi, line2, line3, ok};
use iced::widget::canvas::{self, Path, Stroke, Text};
use iced::{mouse, Color, Point, Rectangle, Size, Theme, Vector};
use shepherd_app_core::pattern_graph::{Graph, Node, NodeKind, NODE_H, NODE_W};

pub struct GraphView<'a> {
    pub graph: &'a Graph,
    /// Key of the selected node, and of the node a connection starts from.
    pub selected: Option<String>,
    pub connecting: Option<String>,
    pub zoom: f32,
    /// The overview map of the inspector: just a box per node.
    pub mini: bool,
}

/// The node under a point of the canvas.
pub fn node_at(graph: &Graph, p: Point) -> Option<&Node> {
    graph
        .nodes
        .iter()
        .rev()
        .find(|n| p.x >= n.x && p.x <= n.x + NODE_W && p.y >= n.y && p.y <= n.y + NODE_H)
}

/// The point where the line from the centre of a `w` x `h` box (centred on `b`) towards `a` leaves the box.
fn clip_to(a: Point, b: Point, w: f32, h: f32) -> Point {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    if dx == 0.0 && dy == 0.0 {
        return b;
    }
    let tx = if dx != 0.0 {
        (w / 2.0 / dx).abs()
    } else {
        f32::INFINITY
    };
    let ty = if dy != 0.0 {
        (h / 2.0 / dy).abs()
    } else {
        f32::INFINITY
    };
    let t = tx.min(ty);
    Point::new(b.x + dx * t, b.y + dy * t)
}

/// The straight arrow between two nodes of the editor: from the border of the sender to the border of the
/// receiver (4 px short of it, so the head does not touch the box).
pub fn straight_edge(from: &Node, to: &Node) -> (Point, Point) {
    let a = Point::new(from.x + NODE_W / 2.0, from.y + NODE_H / 2.0);
    let b = Point::new(to.x + NODE_W / 2.0, to.y + NODE_H / 2.0);
    (
        clip_to(b, a, NODE_W, NODE_H),
        clip_to(a, b, NODE_W + 8.0, NODE_H + 8.0),
    )
}

/// The live topology of a run: the pattern's graph, read-only, each agent coloured by its state and the
/// edges' dashes running while the run is live.
pub struct TopologyView<'a> {
    pub graph: &'a Graph,
    /// `(agent id, role, state colour, state word)` of the run's agents.
    pub agents: Vec<(String, String, Color, String)>,
    pub live: bool,
    pub frame: u32,
}

impl canvas::Program<crate::Message> for TopologyView<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let dx = ((bounds.width - self.graph.width) / 2.0).max(8.0);
        frame.translate(Vector::new(dx, 16.0));
        // a run's agents are matched to the nodes by name, else by role in order (the n-th node of a role is
        // the n-th agent of that role)
        let state_of = |node: &Node| {
            let label = node.label.as_str();
            if let Some(a) = self
                .agents
                .iter()
                .find(|(id, _, _, _)| id == label || id.starts_with(&format!("{label}-")))
            {
                return Some(a);
            }
            let role = node.detail.split(',').next().unwrap_or("").trim();
            let nth = self
                .graph
                .nodes
                .iter()
                .filter(|n| {
                    matches!(n.kind, NodeKind::Agent)
                        && n.detail.split(',').next().unwrap_or("").trim() == role
                })
                .position(|n| n.key == node.key)?;
            self.agents.iter().filter(|(_, r, _, _)| r == role).nth(nth)
        };
        // lines between the centres of the boxes, under them: grey and still, or cyan and flowing while live
        for edge in &self.graph.edges {
            let (from, to) = (&self.graph.nodes[edge.from], &self.graph.nodes[edge.to]);
            let line = Path::line(
                Point::new(from.x + NODE_W / 2.0, from.y + NODE_H / 2.0),
                Point::new(to.x + NODE_W / 2.0, to.y + NODE_H / 2.0),
            );
            if self.live {
                frame.stroke(
                    &line,
                    Stroke {
                        style: canvas::Style::Solid(Color {
                            a: 0.55,
                            ..accent()
                        }),
                        width: 1.5,
                        line_dash: canvas::LineDash {
                            segments: &[4.0, 6.0],
                            // one dash period (10 px) per second, at about 8 frames a second
                            offset: (10 - (self.frame as usize * 10 / 8) % 10) % 10,
                        },
                        ..Stroke::default()
                    },
                );
            } else {
                frame.stroke(
                    &line,
                    Stroke::default()
                        .with_color(crate::theme::line3())
                        .with_width(1.5),
                );
            }
        }
        for node in &self.graph.nodes {
            let is_agent = matches!(node.kind, NodeKind::Agent);
            let state = is_agent.then(|| state_of(node)).flatten();
            let edge_color = state.map_or(line2(), |(_, _, c, _)| *c);
            let rect = Path::rounded_rectangle(
                Point::new(node.x, node.y),
                Size::new(NODE_W, NODE_H),
                6.0.into(),
            );
            frame.fill(
                &rect,
                crate::theme::pick(Color::from_rgb8(0x10, 0x10, 0x18), Color::WHITE),
            );
            let mut stroke = Stroke::default().with_color(edge_color).with_width(1.0);
            if !is_agent {
                stroke.line_dash = canvas::LineDash {
                    segments: &[4.0, 3.0],
                    offset: 0,
                };
                stroke.style = canvas::Style::Solid(crate::theme::violet());
            }
            frame.stroke(&rect, stroke);
            frame.fill_rectangle(
                Point::new(node.x + 12.0, node.y + 14.0),
                Size::new(7.0, 7.0),
                if is_agent {
                    edge_color
                } else {
                    crate::theme::violet()
                },
            );
            frame.fill_text(Text {
                content: node.label.clone(),
                position: Point::new(node.x + 26.0, node.y + 10.0),
                color: hi(),
                size: 13.0.into(),
                ..Text::default()
            });
            let detail = match state {
                Some((_, _, _, word)) => format!("{}, {word}", node.detail),
                None => node.detail.clone(),
            };
            frame.fill_text(Text {
                content: detail.chars().take(26).collect(),
                position: Point::new(node.x + 12.0, node.y + 34.0),
                color: dim(),
                size: 10.5.into(),
                ..Text::default()
            });
        }
        vec![frame.into_geometry()]
    }
}

impl canvas::Program<crate::Message> for GraphView<'_> {
    /// The node being dragged and where inside it the mouse took hold.
    type State = Option<(String, Vector)>;

    fn update(
        &self,
        state: &mut Self::State,
        event: &iced::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<crate::Message>> {
        use crate::panel::PanelMsg;
        use crate::pattern_editor::EditMsg;
        let publish = |m: EditMsg| {
            Some(
                canvas::Action::publish(crate::Message::Panel(PanelMsg::EditPattern(m)))
                    .and_capture(),
            )
        };
        let iced::Event::Mouse(event) = event else {
            return None;
        };
        match event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                let p = cursor.position_in(bounds)?;
                let graph_point = Point::new(p.x / self.zoom, p.y / self.zoom);
                match node_at(self.graph, graph_point) {
                    Some(node) => {
                        *state = Some((
                            node.key.clone(),
                            Vector::new(graph_point.x - node.x, graph_point.y - node.y),
                        ));
                        publish(EditMsg::Press(node.key.clone()))
                    }
                    None => publish(EditMsg::Press(String::new())),
                }
            }
            mouse::Event::CursorMoved { .. } => {
                let (key, offset) = state.as_ref()?;
                let p = cursor.position_in(bounds)?;
                publish(EditMsg::Move(
                    key.clone(),
                    p.x / self.zoom - offset.x,
                    p.y / self.zoom - offset.y,
                ))
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => {
                *state = None;
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        frame.scale(self.zoom);
        // `.graph`: #08080C with a dot every 18 px.
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            crate::theme::pick(
                Color::from_rgb8(0x08, 0x08, 0x0C),
                Color::from_rgb8(0xF3, 0xF3, 0xF7),
            ),
        );
        if self.mini {
            for node in &self.graph.nodes {
                let rect = Path::rounded_rectangle(
                    Point::new(node.x, node.y),
                    Size::new(NODE_W, NODE_H),
                    6.0.into(),
                );
                frame.fill(
                    &rect,
                    crate::theme::pick(
                        Color::from_rgb8(0x15, 0x15, 0x1C),
                        Color::from_rgb8(0xE4, 0xE4, 0xEC),
                    ),
                );
                let marked = self.selected.as_deref() == Some(node.key.as_str());
                frame.stroke(
                    &rect,
                    Stroke::default()
                        .with_color(if marked { accent() } else { line2() })
                        .with_width(if marked { 3.0 } else { 2.0 }),
                );
            }
            return vec![frame.into_geometry()];
        }
        let dot = crate::theme::pick(
            Color::from_rgba8(160, 160, 190, 0.16),
            Color::from_rgba8(90, 90, 120, 0.2),
        );
        for x in (0..bounds.width as u32).step_by(18) {
            for y in (0..bounds.height as u32).step_by(18) {
                frame.fill_rectangle(Point::new(x as f32, y as f32), Size::new(1.0, 1.0), dot);
            }
        }
        // Edges: straight grey lines (1.5 px) from the sender's border to the receiver's, with a grey arrow head
        for edge in &self.graph.edges {
            let (s, d) = straight_edge(&self.graph.nodes[edge.from], &self.graph.nodes[edge.to]);
            frame.stroke(
                &Path::line(s, d),
                Stroke::default().with_color(line3()).with_width(1.5),
            );
            let (dx, dy) = (d.x - s.x, d.y - s.y);
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            let (ux, uy) = (dx / len, dy / len);
            // the marker of the reference: 7 line widths long and wide
            let size = 10.5;
            let base = Point::new(d.x - ux * size, d.y - uy * size);
            let head = Path::new(|p| {
                p.move_to(d);
                p.line_to(Point::new(
                    base.x - uy * size / 2.0,
                    base.y + ux * size / 2.0,
                ));
                p.line_to(Point::new(
                    base.x + uy * size / 2.0,
                    base.y - ux * size / 2.0,
                ));
                p.close();
            });
            frame.fill(&head, line3());
        }
        for node in &self.graph.nodes {
            let marked = self.selected.as_deref() == Some(node.key.as_str());
            let linking = self.connecting.as_deref() == Some(node.key.as_str());
            let origin = Point::new(node.x, node.y);
            let size = Size::new(NODE_W, NODE_H);
            let rect = Path::rounded_rectangle(origin, size, 8.0.into());
            if marked {
                // selection: a 3 px ring, cyan at 18 %
                let ring = Path::rounded_rectangle(
                    Point::new(node.x - 3.0, node.y - 3.0),
                    Size::new(NODE_W + 6.0, NODE_H + 6.0),
                    11.0.into(),
                );
                frame.fill(
                    &ring,
                    Color {
                        a: 0.18,
                        ..accent()
                    },
                );
            }
            frame.fill(&rect, crate::theme::surface());
            let is_agent = matches!(node.kind, NodeKind::Agent);
            let mut stroke = Stroke::default()
                .with_color(if linking {
                    bad()
                } else if marked {
                    accent()
                } else if is_agent {
                    Color { a: 0.4, ..accent() }
                } else {
                    Color {
                        a: 0.6,
                        ..crate::theme::violet()
                    }
                })
                .with_width(1.0);
            if !is_agent {
                stroke.line_dash = canvas::LineDash {
                    segments: &[4.0, 3.0],
                    offset: 0,
                };
            }
            frame.stroke(&rect, stroke);
            // a square in the node's colour, then the name and the line under it
            frame.fill_rectangle(
                Point::new(node.x + 10.0, node.y + 14.0),
                Size::new(7.0, 7.0),
                if is_agent {
                    accent()
                } else {
                    crate::theme::violet()
                },
            );
            frame.fill_text(Text {
                content: node.label.clone(),
                position: Point::new(node.x + 24.0, node.y + 10.0),
                color: hi(),
                size: 12.5.into(),
                ..Text::default()
            });
            frame.fill_text(Text {
                content: node.detail.clone(),
                position: Point::new(node.x + 10.0, node.y + 31.0),
                color: dim(),
                size: 11.0.into(),
                ..Text::default()
            });
            if node.start {
                frame.fill_text(Text {
                    content: "▶".into(),
                    position: Point::new(node.x + NODE_W - 20.0, node.y + 11.0),
                    color: ok(),
                    size: 11.0.into(),
                    ..Text::default()
                });
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(x: f32, y: f32) -> Node {
        Node {
            key: String::new(),
            kind: NodeKind::Agent,
            label: String::new(),
            detail: String::new(),
            start: false,
            x,
            y,
        }
    }

    #[test]
    fn a_press_finds_the_node_under_it() {
        let graph = Graph {
            nodes: vec![
                Node {
                    key: "agent:a".into(),
                    ..node(10.0, 10.0)
                },
                Node {
                    key: "topic:t".into(),
                    ..node(300.0, 10.0)
                },
            ],
            edges: vec![],
            width: 500.0,
            height: 100.0,
        };
        assert_eq!(
            node_at(&graph, Point::new(20.0, 20.0)).map(|n| n.key.as_str()),
            Some("agent:a")
        );
        assert_eq!(
            node_at(&graph, Point::new(310.0, 50.0)).map(|n| n.key.as_str()),
            Some("topic:t")
        );
        assert!(node_at(&graph, Point::new(200.0, 20.0)).is_none());
    }

    #[test]
    fn an_arrow_runs_from_border_to_border_and_stops_short_of_the_target() {
        let (a, b) = straight_edge(&node(0.0, 0.0), &node(300.0, 0.0));
        assert_eq!(a.x, NODE_W, "leaves the right side of the sender");
        assert_eq!(b.x, 300.0 - 4.0, "enters the left side, 4 px short");
        let (a, b) = straight_edge(&node(0.0, 0.0), &node(0.0, 100.0));
        assert_eq!(
            (a.y, b.y),
            (NODE_H, 100.0 - 4.0),
            "same column: bottom to top"
        );
    }
}
