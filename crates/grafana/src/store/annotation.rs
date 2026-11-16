//! Annotations (tables `annotation`, `annotation_tag`, `tag`). Epochs are unix milliseconds.
use super::SResult;

#[derive(Debug, Clone, Default)]
pub struct Annotation {
    pub id: i64,
    pub org_id: i64,
    pub user_id: i64,
    pub dashboard_id: i64,
    pub dashboard_uid: String,
    pub panel_id: i64,
    pub alert_id: i64,
    pub text: String,
    pub epoch_ms: i64,
    pub epoch_end_ms: i64,
    pub tags: Vec<String>,
    /// JSON text.
    pub data: String,
    pub prev_state: String,
    pub new_state: String,
    pub created_ms: i64,
    pub updated_ms: i64,
}

#[derive(Debug, Clone, Default)]
pub struct AnnotationQuery {
    pub org_id: i64,
    /// Overlap window in ms; 0 means unbounded.
    pub from_ms: i64,
    pub to_ms: i64,
    pub dashboard_uid: String,
    pub dashboard_id: i64,
    pub panel_id: i64,
    pub user_id: i64,
    pub alert_id: i64,
    /// "annotation" (no alert) or "alert" or "".
    pub type_: String,
    pub tags: Vec<String>,
    /// With tags: match any tag instead of all.
    pub match_any: bool,
    pub limit: i64,
}

pub trait AnnotationStore {
    /// Inserts and returns the id.
    fn annotation_create(&self, a: &Annotation) -> SResult<i64>;
    /// Writes text, epochs, tags, data and updated by (org_id, id).
    fn annotation_update(&self, a: &Annotation) -> SResult<()>;
    fn annotation_delete(&self, org_id: i64, id: i64) -> SResult<()>;
    fn annotation_by_id(&self, org_id: i64, id: i64) -> SResult<Annotation>;
    /// Newest first (epoch desc).
    fn annotations_find(&self, q: &AnnotationQuery) -> SResult<Vec<Annotation>>;
}
