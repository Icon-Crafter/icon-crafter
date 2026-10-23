//! The document: pages, shared registries, prototype settings and file I/O.
//!
//! `Document` is the root of the persisted design file. It owns every page and
//! the document-wide registries (styles, components, variables, assets) that
//! nodes reference by id. Serialization is plain serde JSON matching
//! `docs/DOCUMENT-FORMAT.md`; the container (`.icraft` ZIP) is handled by
//! `dp-persist`.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ModelError;
use crate::color::Color;
use crate::ids::{
    AssetId, CollectionId, ComponentId, ComponentSetId, DocumentId, FlowId, ModeId, NodeId, PageId,
    StyleId,
};
use crate::node::{Node, NodeKind};
use crate::page::Page;
use crate::style::StyleRegistry;
use crate::variables::VariableRegistry;

/// Current document schema version written by this build.
pub const SCHEMA_VERSION: u32 = 1;

fn default_schema() -> u32 {
    SCHEMA_VERSION
}

fn default_kind() -> DocKind {
    DocKind::Design
}

fn default_now() -> DateTime<Utc> {
    Utc::now()
}

fn default_unit() -> String {
    "px".to_string()
}

fn default_white() -> Color {
    Color::WHITE
}

/// The kind of workspace a document belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    /// Interface design (the main editor).
    #[default]
    Design,
    /// Collaborative whiteboard.
    Whiteboard,
    /// Presentation deck.
    Slides,
    /// Responsive website design.
    Site,
    /// Illustration / drawing workspace.
    Illustration,
}

/// Metadata for a reusable component node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentMeta {
    /// Registry id.
    pub id: ComponentId,
    /// The component node on a page.
    pub node_id: NodeId,
    /// Display name (kept in sync with the node name).
    pub name: String,
    /// Stable key used to match published library components.
    pub key: String,
    /// Optional description shown in the asset panel.
    #[serde(default)]
    pub description: String,
    /// Parent component set, when this component is a variant.
    #[serde(default)]
    pub component_set_id: Option<ComponentSetId>,
    /// Variant property values, e.g. `{"Size": "Large", "State": "Default"}`.
    #[serde(default)]
    pub variant_properties: IndexMap<String, String>,
    /// Shown in published libraries.
    #[serde(default)]
    pub published: bool,
    /// Library id this component was published from.
    #[serde(default)]
    pub library_id: Option<String>,
}

/// Metadata for a component set (group of variants).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentSetMeta {
    /// Registry id.
    pub id: ComponentSetId,
    /// The component set node on a page.
    pub node_id: NodeId,
    /// Display name.
    pub name: String,
    /// Stable library key.
    pub key: String,
    /// Ordered variant property names.
    #[serde(default)]
    pub variant_properties: Vec<String>,
    /// Shown in published libraries.
    #[serde(default)]
    pub published: bool,
}

/// Document-wide component registry.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ComponentRegistry {
    /// Components by id.
    #[serde(default)]
    pub components: IndexMap<ComponentId, ComponentMeta>,
    /// Component sets by id.
    #[serde(default)]
    pub component_sets: IndexMap<ComponentSetId, ComponentSetMeta>,
}

impl ComponentRegistry {
    /// Empty registry.
    pub fn new() -> ComponentRegistry {
        ComponentRegistry::default()
    }

    /// Number of components.
    pub fn len(&self) -> usize {
        self.components.len()
    }

    /// Whether no components are registered.
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    /// Looks up a component.
    pub fn component(&self, id: ComponentId) -> Option<&ComponentMeta> {
        self.components.get(&id)
    }

    /// Whether a component id exists.
    pub fn contains_component(&self, id: ComponentId) -> bool {
        self.components.contains_key(&id)
    }

    /// Looks up a component set.
    pub fn component_set(&self, id: ComponentSetId) -> Option<&ComponentSetMeta> {
        self.component_sets.get(&id)
    }

    /// Inserts or replaces a component.
    pub fn upsert_component(&mut self, meta: ComponentMeta) {
        self.components.insert(meta.id, meta);
    }

    /// Inserts or replaces a component set.
    pub fn upsert_component_set(&mut self, meta: ComponentSetMeta) {
        self.component_sets.insert(meta.id, meta);
    }

    /// Removes a component.
    pub fn remove_component(&mut self, id: ComponentId) -> Option<ComponentMeta> {
        self.components.shift_remove(&id)
    }

    /// Finds the component registered for a node.
    pub fn component_for_node(&self, node_id: NodeId) -> Option<&ComponentMeta> {
        self.components
            .values()
            .find(|meta| meta.node_id == node_id)
    }
}

/// Metadata for an image asset stored alongside the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageAssetMeta {
    /// Asset id.
    pub id: AssetId,
    /// Human readable name (usually the original file name).
    pub name: String,
    /// Content hash (sha256 hex) for deduplication.
    pub hash: String,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// MIME type, e.g. `image/png`.
    pub mime: String,
    /// File name inside the container (`assets/images/<file>`).
    pub file: String,
}

/// Metadata for an embedded font.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontAssetMeta {
    /// Asset id.
    pub id: AssetId,
    /// Font family.
    pub family: String,
    /// Font style name, e.g. `Regular`.
    pub style: String,
    /// Content hash (sha256 hex).
    pub hash: String,
    /// File name inside the container (`assets/fonts/<file>`).
    pub file: String,
    /// Whether the font may be embedded in exports.
    #[serde(default)]
    pub embed: bool,
    /// Numeric weight when known.
    #[serde(default)]
    pub weight: Option<u16>,
    /// Whether the font is italic when known.
    #[serde(default)]
    pub italic: Option<bool>,
}

/// Document-wide asset registry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetRegistry {
    /// Image assets by id.
    #[serde(default)]
    pub images: IndexMap<AssetId, ImageAssetMeta>,
    /// Font assets by id.
    #[serde(default)]
    pub fonts: IndexMap<AssetId, FontAssetMeta>,
}

impl AssetRegistry {
    /// Empty registry.
    pub fn new() -> AssetRegistry {
        AssetRegistry::default()
    }

    /// Looks up an image.
    pub fn image(&self, id: AssetId) -> Option<&ImageAssetMeta> {
        self.images.get(&id)
    }

    /// Looks up a font.
    pub fn font(&self, id: AssetId) -> Option<&FontAssetMeta> {
        self.fonts.get(&id)
    }

    /// Inserts or replaces an image asset.
    pub fn upsert_image(&mut self, meta: ImageAssetMeta) {
        self.images.insert(meta.id, meta);
    }

    /// Inserts or replaces a font asset.
    pub fn upsert_font(&mut self, meta: FontAssetMeta) {
        self.fonts.insert(meta.id, meta);
    }
}

/// Canvas grid configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridSettings {
    /// Horizontal grid spacing in world units.
    #[serde(default = "default_grid_size")]
    pub size_x: f64,
    /// Vertical grid spacing in world units.
    #[serde(default = "default_grid_size")]
    pub size_y: f64,
    /// Whether the grid is drawn.
    #[serde(default)]
    pub visible: bool,
    /// Whether drawing snaps to the grid.
    #[serde(default)]
    pub snap: bool,
    /// Grid origin offset.
    #[serde(default = "default_origin")]
    pub origin: dp_geom::Point,
}

fn default_grid_size() -> f64 {
    8.0
}

fn default_origin() -> dp_geom::Point {
    dp_geom::Point::new(0.0, 0.0)
}

impl Default for GridSettings {
    fn default() -> Self {
        GridSettings {
            size_x: default_grid_size(),
            size_y: default_grid_size(),
            visible: false,
            snap: false,
            origin: default_origin(),
        }
    }
}

/// Per-document editor settings (not page content).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentSettings {
    /// Grid configuration.
    #[serde(default)]
    pub grid: GridSettings,
    /// Whether rulers are shown.
    #[serde(default = "default_true")]
    pub show_rulers: bool,
    /// Whether guides are shown.
    #[serde(default = "default_true")]
    pub show_guides: bool,
    /// Measurement unit label.
    #[serde(default = "default_unit")]
    pub unit: String,
    /// Active variable mode per collection (persisted theme selection).
    #[serde(default)]
    pub active_modes: IndexMap<CollectionId, ModeId>,
}

fn default_true() -> bool {
    true
}

impl Default for DocumentSettings {
    fn default() -> Self {
        DocumentSettings {
            grid: GridSettings::default(),
            show_rulers: true,
            show_guides: true,
            unit: default_unit(),
            active_modes: IndexMap::new(),
        }
    }
}

/// A named prototype flow starting at a frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrototypeFlow {
    /// Flow id.
    pub id: FlowId,
    /// Flow name.
    pub name: String,
    /// Frame the flow starts at.
    pub start_frame_id: NodeId,
    /// Optional description.
    #[serde(default)]
    pub description: String,
}

impl PrototypeFlow {
    /// Creates a flow starting at `start_frame_id`.
    pub fn new(name: impl Into<String>, start_frame_id: NodeId) -> PrototypeFlow {
        PrototypeFlow {
            id: FlowId::new_v4(),
            name: name.into(),
            start_frame_id,
            description: String::new(),
        }
    }
}

/// Device preset used by the prototype viewer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DevicePreset {
    /// Stable preset id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Viewport width in logical pixels.
    pub width: f64,
    /// Viewport height in logical pixels.
    pub height: f64,
}

impl DevicePreset {
    /// iPhone 15 class preset.
    pub fn iphone_15() -> DevicePreset {
        DevicePreset {
            id: "iphone-15".to_string(),
            name: "iPhone 15".to_string(),
            width: 393.0,
            height: 852.0,
        }
    }

    /// Desktop 1440 preset.
    pub fn desktop() -> DevicePreset {
        DevicePreset {
            id: "desktop".to_string(),
            name: "Desktop".to_string(),
            width: 1440.0,
            height: 900.0,
        }
    }
}

/// Document-level prototype configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrototypeSettings {
    /// Frames that can be used as prototype start points.
    #[serde(default)]
    pub start_frames: Vec<NodeId>,
    /// Named flows.
    #[serde(default)]
    pub flows: Vec<PrototypeFlow>,
    /// Viewer device preset.
    #[serde(default)]
    pub device: Option<DevicePreset>,
    /// Presentation background.
    #[serde(default = "default_white")]
    pub background: Color,
}

impl Default for PrototypeSettings {
    fn default() -> Self {
        PrototypeSettings {
            start_frames: Vec::new(),
            flows: Vec::new(),
            device: Some(DevicePreset::iphone_15()),
            background: Color::WHITE,
        }
    }
}

/// The root of a design file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Schema version of this payload.
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// Document id (stable across saves).
    pub id: DocumentId,
    /// File name (without extension).
    pub name: String,
    /// Workspace kind.
    #[serde(default = "default_kind")]
    pub kind: DocKind,
    /// Creation timestamp.
    #[serde(default = "default_now")]
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp.
    #[serde(default = "default_now")]
    pub updated_at: DateTime<Utc>,
    /// Monotonic revision counter, bumped for every committed transaction.
    #[serde(default = "crate::rev_one")]
    pub revision: u64,
    /// Currently open page.
    pub active_page: PageId,
    /// Pages in tab order.
    #[serde(default)]
    pub pages: Vec<Page>,
    /// Shared style registry.
    #[serde(default)]
    pub styles: StyleRegistry,
    /// Component registry.
    #[serde(default)]
    pub components: ComponentRegistry,
    /// Variable collections and modes.
    #[serde(default)]
    pub variables: VariableRegistry,
    /// Image and font assets.
    #[serde(default)]
    pub assets: AssetRegistry,
    /// Prototype settings.
    #[serde(default)]
    pub prototype: PrototypeSettings,
    /// Editor settings.
    #[serde(default)]
    pub settings: DocumentSettings,
}

impl Default for Document {
    fn default() -> Self {
        Document::new("Untitled")
    }
}

impl Document {
    /// Schema version understood by this build.
    pub const SCHEMA_VERSION: u32 = SCHEMA_VERSION;

    /// Creates a document with one empty page named `Page 1`.
    pub fn new(name: impl Into<String>) -> Document {
        Document::new_with_kind(name, DocKind::Design)
    }

    /// Creates a document of a specific workspace kind.
    pub fn new_with_kind(name: impl Into<String>, kind: DocKind) -> Document {
        let page = Page::new("Page 1");
        let now = Utc::now();
        Document {
            schema: SCHEMA_VERSION,
            id: DocumentId::new_v4(),
            name: name.into(),
            kind,
            created_at: now,
            updated_at: now,
            revision: 1,
            active_page: page.id,
            pages: vec![page],
            styles: StyleRegistry::default(),
            components: ComponentRegistry::default(),
            variables: VariableRegistry::default(),
            assets: AssetRegistry::default(),
            prototype: PrototypeSettings::default(),
            settings: DocumentSettings::default(),
        }
    }

    /// The active page (falls back to the first page if the id is stale).
    pub fn active_page(&self) -> &Page {
        self.pages
            .iter()
            .find(|page| page.id == self.active_page)
            .unwrap_or_else(|| {
                self.pages
                    .first()
                    .expect("document invariant: at least one page")
            })
    }

    /// Mutable active page.
    pub fn active_page_mut(&mut self) -> &mut Page {
        let index = self
            .pages
            .iter()
            .position(|page| page.id == self.active_page)
            .unwrap_or(0);
        &mut self.pages[index]
    }

    /// Id of the active page.
    pub fn active_page_id(&self) -> PageId {
        self.active_page().id
    }

    /// Looks up a page.
    pub fn page(&self, id: PageId) -> Option<&Page> {
        self.pages.iter().find(|page| page.id == id)
    }

    /// Mutably looks up a page.
    pub fn page_mut(&mut self, id: PageId) -> Option<&mut Page> {
        self.pages.iter_mut().find(|page| page.id == id)
    }

    /// Page index in tab order.
    pub fn page_index(&self, id: PageId) -> Option<usize> {
        self.pages.iter().position(|page| page.id == id)
    }

    /// Switches the active page.
    pub fn set_active_page(&mut self, id: PageId) -> Result<(), ModelError> {
        if self.page(id).is_none() {
            return Err(ModelError::PageNotFound(id));
        }
        self.active_page = id;
        Ok(())
    }

    /// Appends a new empty page and returns its id.
    pub fn add_page(&mut self, name: impl Into<String>) -> PageId {
        let page = Page::new(name);
        let id = page.id;
        self.pages.push(page);
        id
    }

    /// Removes a page; refuses to remove the last remaining page.
    pub fn remove_page(&mut self, id: PageId) -> Result<Page, ModelError> {
        if self.pages.len() <= 1 {
            return Err(ModelError::InvalidDocument(
                "a document must keep at least one page".to_string(),
            ));
        }
        let index = self.page_index(id).ok_or(ModelError::PageNotFound(id))?;
        let page = self.pages.remove(index);
        if self.active_page == id {
            let next = index.min(self.pages.len() - 1);
            self.active_page = self.pages[next].id;
        }
        Ok(page)
    }

    /// Total number of nodes across all pages.
    pub fn node_count(&self) -> usize {
        self.pages.iter().map(Page::len).sum()
    }

    /// Looks up a node anywhere in the document.
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.pages.iter().find_map(|page| page.get(id))
    }

    /// Mutably looks up a node anywhere in the document.
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.pages.iter_mut().find_map(|page| page.get_mut(id))
    }

    /// Finds the page containing a node.
    pub fn page_containing(&self, id: NodeId) -> Option<PageId> {
        self.pages
            .iter()
            .find(|page| page.contains(id))
            .map(|page| page.id)
    }

    /// Looks up a node together with its page.
    pub fn locate_node(&self, id: NodeId) -> Option<(&Page, &Node)> {
        self.pages
            .iter()
            .find_map(|page| page.get(id).map(|node| (page, node)))
    }

    /// Marks the document as modified (revision + timestamp).
    pub fn touch(&mut self) {
        self.revision = self.revision.saturating_add(1);
        self.updated_at = Utc::now();
    }

    /// Serializes to a JSON value.
    pub fn to_json_value(&self) -> Result<Value, ModelError> {
        serde_json::to_value(self).map_err(|error| ModelError::Json(error.to_string()))
    }

    /// Serializes to a JSON string with canonicalised key ordering.
    ///
    /// Going through [`serde_json::Value`] makes the byte output stable: two
    /// documents with the same content always serialize identically, which the
    /// `.icraft` container and content addressing rely on.
    pub fn to_json_string(&self) -> Result<String, ModelError> {
        serde_json::to_string(&self.to_json_value()?)
            .map_err(|error| ModelError::Json(error.to_string()))
    }

    /// Parses and validates a document from a JSON string.
    pub fn from_json_str(text: &str) -> Result<Document, ModelError> {
        let value: Value =
            serde_json::from_str(text).map_err(|error| ModelError::Json(error.to_string()))?;
        Document::from_json_value(value)
    }

    /// Parses and validates a document from a JSON value, applying migrations.
    pub fn from_json_value(mut value: Value) -> Result<Document, ModelError> {
        let object = value
            .as_object_mut()
            .ok_or_else(|| ModelError::InvalidDocument("document must be a JSON object".into()))?;
        let schema = object
            .get("schema")
            .and_then(Value::as_u64)
            .ok_or_else(|| ModelError::InvalidDocument("document is missing schema".into()))?
            as u32;
        if schema > SCHEMA_VERSION {
            return Err(ModelError::UnsupportedSchema {
                found: schema,
                supported: SCHEMA_VERSION,
            });
        }
        migrate_to_current(&mut value, schema)?;
        let mut document: Document = serde_json::from_value(value)
            .map_err(|error| ModelError::InvalidDocument(error.to_string()))?;
        document.schema = SCHEMA_VERSION;
        if document.pages.is_empty() {
            return Err(ModelError::InvalidDocument(
                "document has no pages".to_string(),
            ));
        }
        if !document
            .pages
            .iter()
            .any(|page| page.id == document.active_page)
        {
            document.active_page = document.pages[0].id;
        }
        for page in &document.pages {
            let issues = page.check_integrity();
            if !issues.is_empty() {
                return Err(ModelError::InvalidDocument(format!(
                    "page '{}' is corrupt: {}",
                    page.name,
                    issues.join("; ")
                )));
            }
        }
        Ok(document)
    }

    /// Validates the document, returning every issue found (errors and warnings).
    pub fn validate(&self) -> Vec<crate::validation::ValidationIssue> {
        crate::validation::validate(self)
    }

    /// Sorted list of page/node pairs for stable iteration.
    pub fn node_ids(&self) -> Vec<NodeId> {
        self.pages
            .iter()
            .flat_map(|page| page.nodes.keys().copied())
            .collect()
    }

    /// All component node ids (kind `Component`).
    pub fn component_node_ids(&self) -> Vec<NodeId> {
        self.pages
            .iter()
            .flat_map(|page| page.nodes.values())
            .filter(|node| node.kind == NodeKind::Component)
            .map(|node| node.id)
            .collect()
    }

    /// Shared style ids referenced by a node.
    pub fn node_style_refs(&self, node: &Node) -> Vec<StyleId> {
        let mut refs = Vec::new();
        for fill in &node.style.fills {
            if let Some(id) = fill.style_id {
                refs.push(id);
            }
        }
        for stroke in &node.style.strokes {
            if let Some(id) = stroke.style_id {
                refs.push(id);
            }
        }
        refs
    }

    /// Ensures node ids are unique across pages.
    pub fn unique_node_ids(&self) -> Result<(), ModelError> {
        let mut seen: HashSet<NodeId> = HashSet::new();
        for page in &self.pages {
            for id in page.nodes.keys() {
                if !seen.insert(*id) {
                    return Err(ModelError::InvalidDocument(format!(
                        "node id {id} appears on more than one page"
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Applies the migration chain for older schemas. Currently only v1 exists.
fn migrate_to_current(value: &mut Value, from: u32) -> Result<(), ModelError> {
    // v1 is the first schema; older documents cannot be migrated safely, so we
    // reject them instead of guessing at missing properties.
    if from < SCHEMA_VERSION {
        return Err(ModelError::UnsupportedSchema {
            found: from,
            supported: SCHEMA_VERSION,
        });
    }
    let object = value
        .as_object_mut()
        .ok_or_else(|| ModelError::InvalidDocument("document must be a JSON object".into()))?;
    object.insert("schema".to_string(), Value::from(SCHEMA_VERSION));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::FillRule;
    use crate::node::Transform;
    use crate::paint::Fill;

    fn sample_document() -> (Document, NodeId) {
        let mut document = Document::new("Sample");
        let page_id = document.active_page_id();
        let frame = Node::frame("Frame", 10.0, 20.0, 320.0, 200.0);
        let frame_id = frame.id;
        document
            .page_mut(page_id)
            .unwrap()
            .insert_node(frame, None, None)
            .unwrap();
        let mut rect = Node::rectangle("Rectangle", 40.0, 60.0, 120.0, 80.0);
        rect.style.fills = vec![Fill::solid(Color::from_hex("#ff6b4a").unwrap())];
        document
            .page_mut(page_id)
            .unwrap()
            .insert_node(rect, Some(frame_id), None)
            .unwrap();
        (document, frame_id)
    }

    #[test]
    fn new_document_has_one_page() {
        let document = Document::new("Untitled");
        assert_eq!(document.pages.len(), 1);
        assert_eq!(document.schema, SCHEMA_VERSION);
        assert_eq!(document.revision, 1);
        assert_eq!(document.active_page_id(), document.pages[0].id);
        assert_eq!(document.kind, DocKind::Design);
    }

    #[test]
    fn json_roundtrip_preserves_document() {
        let (document, _) = sample_document();
        let json = document.to_json_string().unwrap();
        let parsed = Document::from_json_str(&json).unwrap();
        assert_eq!(parsed.name, document.name);
        assert_eq!(parsed.id, document.id);
        assert_eq!(parsed.pages.len(), document.pages.len());
        assert_eq!(parsed.node_count(), document.node_count());
        assert_eq!(parsed.revision, document.revision);
        // Round-trip is stable.
        assert_eq!(parsed.to_json_string().unwrap(), json);
    }

    #[test]
    fn newer_schema_is_rejected() {
        let (document, _) = sample_document();
        let mut value = document.to_json_value().unwrap();
        value["schema"] = Value::from(999u32);
        let error = Document::from_json_value(value).unwrap_err();
        assert!(matches!(
            error,
            ModelError::UnsupportedSchema { found: 999, .. }
        ));
    }

    #[test]
    fn invalid_documents_are_rejected() {
        let (document, _) = sample_document();
        let mut value = document.to_json_value().unwrap();
        value.as_object_mut().unwrap().remove("schema");
        assert!(matches!(
            Document::from_json_value(value.clone()).unwrap_err(),
            ModelError::InvalidDocument(_)
        ));

        let mut value = document.to_json_value().unwrap();
        value["pages"] = Value::Array(Vec::new());
        assert!(matches!(
            Document::from_json_value(value).unwrap_err(),
            ModelError::InvalidDocument(_)
        ));
    }

    #[test]
    fn page_management_respects_last_page() {
        let mut document = Document::new("Pages");
        let first = document.active_page_id();
        let second = document.add_page("Page 2");
        document.set_active_page(second).unwrap();
        assert_eq!(document.active_page().name, "Page 2");
        let removed = document.remove_page(second).unwrap();
        assert_eq!(removed.name, "Page 2");
        assert_eq!(document.active_page_id(), first);
        assert!(document.remove_page(first).is_err());
    }

    #[test]
    fn find_and_mutate_nodes_across_pages() {
        let (mut document, frame_id) = sample_document();
        let second = document.add_page("Other");
        let page = document.page_mut(second).unwrap();
        let rect = Node::rectangle("On second page", 0.0, 0.0, 10.0, 10.0);
        let rect_id = rect.id;
        page.insert_node(rect, None, None).unwrap();
        assert_eq!(
            document.page_containing(frame_id),
            Some(document.active_page_id())
        );
        assert_eq!(document.page_containing(rect_id), Some(second));
        {
            let node = document.node_mut(rect_id).unwrap();
            node.name = "Renamed".to_string();
        }
        assert_eq!(document.node(rect_id).unwrap().name, "Renamed");
    }

    #[test]
    fn style_refs_and_component_helpers() {
        let (document, frame_id) = sample_document();
        let node = document.node(frame_id).unwrap();
        assert!(document.node_style_refs(node).is_empty());
        assert!(document.component_node_ids().is_empty());
        assert_eq!(document.node_ids().len(), 2);
        assert_eq!(document.kind, DocKind::Design);
    }

    #[test]
    fn document_roundtrip_with_geometry_and_kind() {
        let mut document = Document::new_with_kind("Site", DocKind::Site);
        let page_id = document.active_page_id();
        let star = Node::new(
            NodeKind::Star,
            "Star",
            Transform::new(0.0, 0.0, 100.0, 100.0),
        )
        .with_geometry(crate::geometry::NodeGeometry::Star {
            point_count: 5,
            inner_ratio: 0.38,
        });
        document
            .page_mut(page_id)
            .unwrap()
            .insert_node(star, None, None)
            .unwrap();
        let json = document.to_json_string().unwrap();
        assert!(json.contains("\"star\""));
        let parsed = Document::from_json_str(&json).unwrap();
        assert_eq!(parsed.kind, DocKind::Site);
        assert_eq!(parsed.node_count(), 1);
        assert!(parsed.validate().iter().all(|issue| issue.is_warning()));
        let _ = FillRule::NonZero;
    }

    #[test]
    fn unique_node_ids_detects_duplicates() {
        let (mut document, frame_id) = sample_document();
        // Insert a copy of the frame on another page with the same id.
        let second = document.add_page("Dup");
        let clone = document.node(frame_id).unwrap().clone();
        let page = document.page_mut(second).unwrap();
        // Simulate corruption by inserting directly into the map.
        let mut clone2 = clone.clone();
        clone2.parent = None;
        clone2.children.clear();
        page.nodes.insert(frame_id, clone2);
        page.roots.push(frame_id);
        assert!(document.unique_node_ids().is_err());
    }
}
