//! Nodes: the single editable object type of the document.
//!
//! Every object in the editor - frames, shapes, text, components, instances,
//! whiteboard stickies - is a [`Node`] with a [`NodeKind`] and optional
//! property groups. This keeps selection, transforms, undo, rendering and
//! collaboration uniform across object types.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::geometry::{AutoLayout, Constraints, NodeGeometry, TextContent};
use crate::ids::{ComponentId, NodeId};
use crate::paint::BlendMode;
use crate::prototype::Interaction;
use crate::style::NodeStyle;
use dp_geom::{Mat2D, Point, Rect, Size};

/// The kind of object a node represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A frame (artboard).
    Frame,
    /// A group.
    Group,
    /// A component definition.
    Component,
    /// A component set (variant container).
    ComponentSet,
    /// An instance of a component.
    Instance,
    /// A rectangle.
    Rectangle,
    /// An ellipse.
    Ellipse,
    /// A line.
    Line,
    /// A regular polygon.
    Polygon,
    /// A star.
    Star,
    /// An editable vector path.
    Vector,
    /// A text layer.
    Text,
    /// An image layer.
    Image,
    /// A boolean combination of children.
    Boolean,
    /// An export slice.
    Slice,
    /// A whiteboard sticky note.
    Sticky,
    /// A whiteboard connector.
    Connector,
}

impl NodeKind {
    /// Whether the node may contain children.
    pub fn is_container(self) -> bool {
        matches!(
            self,
            NodeKind::Frame
                | NodeKind::Group
                | NodeKind::Component
                | NodeKind::ComponentSet
                | NodeKind::Instance
                | NodeKind::Boolean
                | NodeKind::Slice
        )
    }

    /// Whether the node is a frame-like clipping container.
    pub fn clips_content(self) -> bool {
        matches!(
            self,
            NodeKind::Frame | NodeKind::Component | NodeKind::ComponentSet
        )
    }

    /// Human readable label used by the layers panel and generated code.
    pub fn label(self) -> &'static str {
        match self {
            NodeKind::Frame => "Frame",
            NodeKind::Group => "Group",
            NodeKind::Component => "Component",
            NodeKind::ComponentSet => "Component set",
            NodeKind::Instance => "Instance",
            NodeKind::Rectangle => "Rectangle",
            NodeKind::Ellipse => "Ellipse",
            NodeKind::Line => "Line",
            NodeKind::Polygon => "Polygon",
            NodeKind::Star => "Star",
            NodeKind::Vector => "Vector",
            NodeKind::Text => "Text",
            NodeKind::Image => "Image",
            NodeKind::Boolean => "Boolean",
            NodeKind::Slice => "Slice",
            NodeKind::Sticky => "Sticky note",
            NodeKind::Connector => "Connector",
        }
    }

    /// Whether the node is a shape (has editable geometry).
    pub fn is_shape(self) -> bool {
        matches!(
            self,
            NodeKind::Rectangle
                | NodeKind::Ellipse
                | NodeKind::Line
                | NodeKind::Polygon
                | NodeKind::Star
                | NodeKind::Vector
                | NodeKind::Boolean
        )
    }
}

/// The transform of a node inside its parent.
///
/// Rotations are stored in degrees and always happen about the node's centre.
/// Flips are applied before rotation, matching the behaviour designers expect
/// from design tools.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// Left position in the parent's coordinate space.
    pub x: f64,
    /// Top position in the parent's coordinate space.
    pub y: f64,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
    /// Rotation in degrees, clockwise.
    #[serde(default)]
    pub rotation: f64,
    /// Mirror horizontally.
    #[serde(default)]
    pub flip_x: bool,
    /// Mirror vertically.
    #[serde(default)]
    pub flip_y: bool,
}

impl Default for Transform {
    fn default() -> Transform {
        Transform::new(0.0, 0.0, 100.0, 100.0)
    }
}

impl Transform {
    /// Creates a transform.
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Transform {
        Transform {
            x,
            y,
            w,
            h,
            rotation: 0.0,
            flip_x: false,
            flip_y: false,
        }
    }

    /// A transform at the origin with the given size.
    pub const fn at_origin(size: Size) -> Transform {
        Transform::new(0.0, 0.0, size.w, size.h)
    }

    /// The bounds of the untransformed box in parent space.
    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.w, self.h)
    }

    /// The size.
    pub fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }

    /// The centre in parent space (ignoring rotation, which is about it).
    pub fn center(&self) -> Point {
        Point::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    /// Sets the position.
    pub fn set_position(&mut self, x: f64, y: f64) {
        self.x = x;
        self.y = y;
    }

    /// Translates by a delta.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }

    /// Sets the size, never allowing negative dimensions.
    pub fn set_size(&mut self, w: f64, h: f64) {
        self.w = w.max(0.0);
        self.h = h.max(0.0);
    }

    /// The local-to-parent matrix.
    pub fn matrix(&self) -> Mat2D {
        let center = self.center();
        let flip = Mat2D::scale(
            if self.flip_x { -1.0 } else { 1.0 },
            if self.flip_y { -1.0 } else { 1.0 },
        );
        Mat2D::translation(center.x, center.y)
            .mul(Mat2D::rotation(self.rotation.to_radians()))
            .mul(flip)
            .mul(Mat2D::translation(-self.w / 2.0, -self.h / 2.0))
    }

    /// The local-to-parent matrix with the box origin at `(0,0)` (used when
    /// rendering a node's own geometry).
    pub fn local_matrix(&self) -> Mat2D {
        self.matrix()
    }

    /// Axis aligned bounds in parent space after rotation.
    pub fn bounds(&self) -> Rect {
        self.matrix().apply_rect(Rect::new(0.0, 0.0, self.w, self.h))
    }

    /// Whether the transform contains only finite values.
    pub fn is_finite(&self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.w.is_finite()
            && self.h.is_finite()
            && self.rotation.is_finite()
    }
}

/// An override applied to a node inside a component instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceOverride {
    /// The node inside the component that is overridden.
    pub target: NodeId,
    /// The property name (`characters`, `visible`, `fill.0.paint`, ...).
    pub prop: String,
    /// The overridden value.
    pub value: serde_json::Value,
}

/// Component related data attached to component/instance nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ComponentRole {
    /// This node publishes a component.
    Component {
        /// Stable component key shared by library consumers.
        key: String,
        /// Optional description shown in the library.
        #[serde(default)]
        description: String,
        /// Variant property values, e.g. `size: "large"`.
        #[serde(default)]
        variant_properties: IndexMap<String, String>,
    },
    /// This node groups component variants into a set.
    ComponentSet {
        /// Stable component set key.
        key: String,
    },
    /// This node is an instance of a component.
    Instance {
        /// The component being instantiated.
        component_id: ComponentId,
        /// Local overrides.
        #[serde(default)]
        overrides: Vec<InstanceOverride>,
        /// Component swapped through an instance-swap property.
        #[serde(default)]
        swapped_component_id: Option<ComponentId>,
    },
}

/// Prototype data attached to a node.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PrototypeNode {
    /// Interactions starting on this node.
    #[serde(default)]
    pub interactions: Vec<Interaction>,
}

/// A node in a page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Unique id (unique across the whole document).
    pub id: NodeId,
    /// Display name.
    pub name: String,
    /// Object kind.
    pub kind: NodeKind,
    /// Parent node, `None` for page roots.
    #[serde(default)]
    pub parent: Option<NodeId>,
    /// Direct children, bottom-most first (paint order).
    #[serde(default)]
    pub children: Vec<NodeId>,
    /// Revision counter bumped on every edit (used by collaboration/debugging).
    #[serde(default = "crate::rev_one")]
    pub rev: u64,
    /// Whether the node is painted.
    #[serde(default = "crate::truth")]
    pub visible: bool,
    /// Whether the node cannot be selected on the canvas.
    #[serde(default)]
    pub locked: bool,
    /// Layer opacity, multiplied with fill/stroke opacity.
    #[serde(default = "crate::one")]
    pub opacity: f64,
    /// Layer blend mode.
    #[serde(default)]
    pub blend: BlendMode,
    /// Position/size/rotation in parent space.
    pub transform: Transform,
    /// Paint properties.
    #[serde(default)]
    pub style: NodeStyle,
    /// Shape geometry (shapes and boolean ops only).
    #[serde(default)]
    pub geometry: Option<NodeGeometry>,
    /// Text content (text nodes only).
    #[serde(default)]
    pub text: Option<TextContent>,
    /// Auto layout (containers only).
    #[serde(default)]
    pub layout: Option<AutoLayout>,
    /// Resize constraints.
    #[serde(default)]
    pub constraints: Option<Constraints>,
    /// Component data.
    #[serde(default)]
    pub component: Option<ComponentRole>,
    /// Prototype data.
    #[serde(default)]
    pub prototype: Option<PrototypeNode>,
    /// Free-form plugin/engine properties.
    #[serde(default)]
    pub props: IndexMap<String, serde_json::Value>,
    /// Plugin private data (namespaced by plugin id).
    #[serde(default)]
    pub plugin_data: IndexMap<String, String>,
}

impl Node {
    /// Creates a node with the given kind, name and transform.
    pub fn new(kind: NodeKind, name: impl Into<String>, transform: Transform) -> Node {
        Node {
            id: NodeId::new_v4(),
            name: name.into(),
            kind,
            parent: None,
            children: Vec::new(),
            rev: 1,
            visible: true,
            locked: false,
            opacity: 1.0,
            blend: BlendMode::Normal,
            transform,
            style: NodeStyle::default(),
            geometry: None,
            text: None,
            layout: None,
            constraints: None,
            component: None,
            prototype: None,
            props: IndexMap::new(),
            plugin_data: IndexMap::new(),
        }
    }

    /// Creates a node with an explicit id (used by importers and tests).
    pub fn with_id(id: NodeId, kind: NodeKind, name: impl Into<String>, transform: Transform) -> Node {
        Node {
            id,
            ..Node::new(kind, name, transform)
        }
    }

    /// Convenience constructor for a rectangular shape.
    pub fn rectangle(name: impl Into<String>, x: f64, y: f64, w: f64, h: f64) -> Node {
        Node::new(NodeKind::Rectangle, name, Transform::new(x, y, w, h))
    }

    /// Convenience constructor for a frame.
    pub fn frame(name: impl Into<String>, x: f64, y: f64, w: f64, h: f64) -> Node {
        Node::new(NodeKind::Frame, name, Transform::new(x, y, w, h))
    }

    /// Convenience constructor for a text node.
    pub fn text(name: impl Into<String>, characters: impl Into<String>, x: f64, y: f64) -> Node {
        let mut node = Node::new(NodeKind::Text, name, Transform::new(x, y, 200.0, 24.0));
        node.text = Some(TextContent::plain(characters));
        node
    }

    /// Attaches geometry.
    pub fn with_geometry(mut self, geometry: NodeGeometry) -> Node {
        self.geometry = Some(geometry);
        self
    }

    /// Attaches a style.
    pub fn with_style(mut self, style: NodeStyle) -> Node {
        self.style = style;
        self
    }

    /// Attaches auto layout.
    pub fn with_layout(mut self, layout: AutoLayout) -> Node {
        self.layout = Some(layout);
        self
    }

    /// Whether the node is a container.
    pub fn is_container(&self) -> bool {
        self.kind.is_container()
    }

    /// Whether the node is a component definition.
    pub fn is_component(&self) -> bool {
        matches!(self.kind, NodeKind::Component)
    }

    /// Whether the node is an instance.
    pub fn is_instance(&self) -> bool {
        matches!(self.kind, NodeKind::Instance)
    }

    /// Effective opacity clamped into `0..=1`.
    pub fn effective_opacity(&self) -> f64 {
        self.opacity.clamp(0.0, 1.0)
    }

    /// Absolute position of the node's box origin (ignores rotation).
    pub fn position(&self) -> Point {
        Point::new(self.transform.x, self.transform.y)
    }

    /// Component id of an instance, if any.
    pub fn instance_component(&self) -> Option<ComponentId> {
        match &self.component {
            Some(ComponentRole::Instance { component_id, .. }) => Some(*component_id),
            _ => None,
        }
    }

    /// Applies a closure to the text content if this is a text node.
    pub fn text_mut(&mut self) -> Option<&mut TextContent> {
        self.text.as_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dp_geom::Point;

    #[test]
    fn transform_matrix_places_box_and_rotates_about_center() {
        let transform = Transform::new(10.0, 20.0, 100.0, 50.0);
        let matrix = transform.matrix();
        let origin = matrix.apply_point(Point::new(0.0, 0.0));
        assert!((origin.x - 10.0).abs() < 1e-9);
        assert!((origin.y - 20.0).abs() < 1e-9);
        let corner = matrix.apply_point(Point::new(100.0, 50.0));
        assert!((corner.x - 110.0).abs() < 1e-9);
        assert!((corner.y - 70.0).abs() < 1e-9);

        // Rotation about the centre keeps the centre fixed.
        let rotated = Transform {
            rotation: 90.0,
            ..transform
        };
        let center = rotated.center();
        let mapped = rotated.matrix().apply_point(Point::new(50.0, 25.0));
        assert!((mapped.x - center.x).abs() < 1e-9);
        assert!((mapped.y - center.y).abs() < 1e-9);
    }

    #[test]
    fn node_serde_roundtrip() {
        let mut node = Node::rectangle("Card", 10.0, 20.0, 200.0, 120.0);
        node.style = crate::style::NodeStyle::filled(crate::color::Color::WHITE);
        node.props.insert("data-id".into(), serde_json::json!("card-1"));
        let json = serde_json::to_value(&node).unwrap();
        assert_eq!(json["kind"], "rectangle");
        assert_eq!(json["transform"]["w"], 200.0);
        let back: Node = serde_json::from_value(json).unwrap();
        assert_eq!(back, node);
    }

    #[test]
    fn serde_defaults_keep_backwards_compatibility() {
        let json = serde_json::json!({
            "id": NodeId::new_v4(),
            "name": "Old node",
            "kind": "group",
            "transform": {"x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0}
        });
        let node: Node = serde_json::from_value(json).unwrap();
        assert!(node.visible);
        assert_eq!(node.opacity, 1.0);
        assert_eq!(node.rev, 1);
        assert!(node.children.is_empty());
    }

    #[test]
    fn kind_helpers() {
        assert!(NodeKind::Frame.is_container());
        assert!(NodeKind::Boolean.is_container());
        assert!(!NodeKind::Rectangle.is_container());
        assert!(NodeKind::Frame.clips_content());
        assert!(!NodeKind::Group.clips_content());
        assert!(NodeKind::Star.is_shape());
    }
}
