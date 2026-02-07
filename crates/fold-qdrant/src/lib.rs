//! Qdrant service for vector storage.
//!
//! Provides collection management, upsert, search, and delete operations
//! for storing and retrieving memory embeddings.

use std::collections::HashMap;
use std::sync::Arc;

use qdrant_client::qdrant::{
    condition::ConditionOneOf, r#match::MatchValue, Condition, CreateCollectionBuilder,
    DeletePointsBuilder, Distance, FieldCondition, Filter, Match, PointId, PointStruct,
    ScoredPoint, ScrollPointsBuilder, SearchPointsBuilder, UpsertPointsBuilder,
    Value as QdrantValue, VectorParamsBuilder,
};
use qdrant_client::Qdrant;
use serde_json::Value;
use tracing::{debug, info};

/// Error types for the Qdrant service.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Vector store error: {0}")]
    VectorStore(String),
}

/// Result type for the Qdrant service.
pub type Result<T> = std::result::Result<T, Error>;

/// Configuration for the Qdrant service.
#[derive(Debug, Clone)]
pub struct QdrantConfig {
    pub url: String,
    pub collection_prefix: String,
}

impl QdrantConfig {
    /// Create a new Qdrant configuration.
    pub fn new(url: impl Into<String>, collection_prefix: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            collection_prefix: collection_prefix.into(),
        }
    }
}

/// Point payload key names
const KEY_TYPE: &str = "type";
const KEY_AUTHOR: &str = "author";
const KEY_FILE_PATH: &str = "file_path";
const KEY_PROJECT_ID: &str = "project_id";

/// Service for vector storage using Qdrant.
///
/// Manages collections per project with format: `{prefix}{project_slug}`
#[derive(Clone)]
pub struct QdrantService {
    inner: Arc<QdrantServiceInner>,
}

struct QdrantServiceInner {
    client: Qdrant,
    prefix: String,
}

/// Search result from Qdrant
#[derive(Debug, Clone)]
pub struct VectorSearchResult {
    pub id: String,
    pub score: f32,
    pub payload: HashMap<String, Value>,
}

impl QdrantService {
    /// Create a new Qdrant service.
    pub async fn new(config: &QdrantConfig) -> Result<Self> {
        let client = Qdrant::from_url(&config.url)
            .build()
            .map_err(|e| Error::VectorStore(format!("Failed to connect to Qdrant: {}", e)))?;

        // Test connection
        client
            .list_collections()
            .await
            .map_err(|e| Error::VectorStore(format!("Qdrant connection test failed: {}", e)))?;

        info!(url = %config.url, prefix = %config.collection_prefix, "Qdrant service connected");

        Ok(Self {
            inner: Arc::new(QdrantServiceInner {
                client,
                prefix: config.collection_prefix.clone(),
            }),
        })
    }

    /// Get the collection name for a project
    pub fn collection_name(&self, project_slug: &str) -> String {
        format!("{}{}", self.inner.prefix, project_slug)
    }

    /// Create a collection for a project if it doesn't exist.
    /// If the collection exists but has a different dimension, it will be
    /// deleted and recreated with the correct dimension.
    pub async fn create_collection(&self, project_slug: &str, dimension: usize) -> Result<()> {
        let collection_name = self.collection_name(project_slug);

        // Check if collection exists
        let exists = self
            .inner
            .client
            .collection_exists(&collection_name)
            .await
            .map_err(|e| Error::VectorStore(format!("Failed to check collection: {}", e)))?;

        if exists {
            // Check if the existing collection has the right dimension
            let info = self
                .inner
                .client
                .collection_info(&collection_name)
                .await
                .map_err(|e| Error::VectorStore(format!("Failed to get collection info: {}", e)))?;

            // Extract vector dimension from collection config
            let existing_dim = info
                .result
                .as_ref()
                .and_then(|r| r.config.as_ref())
                .and_then(|c| c.params.as_ref())
                .and_then(|p| p.vectors_config.as_ref())
                .and_then(|vc| match vc.config.as_ref() {
                    Some(qdrant_client::qdrant::vectors_config::Config::Params(params)) => {
                        Some(params.size as usize)
                    }
                    _ => None,
                })
                .unwrap_or(0);

            if existing_dim == dimension {
                debug!(collection = %collection_name, dimension, "Collection already exists with correct dimension");
                return Ok(());
            }

            // Dimension mismatch - delete and recreate
            info!(
                collection = %collection_name,
                existing_dim,
                new_dim = dimension,
                "Collection dimension mismatch - recreating"
            );

            self.inner
                .client
                .delete_collection(&collection_name)
                .await
                .map_err(|e| Error::VectorStore(format!("Failed to delete mismatched collection: {}", e)))?;
        }

        // Create collection using builder
        self.inner
            .client
            .create_collection(
                CreateCollectionBuilder::new(&collection_name)
                    .vectors_config(VectorParamsBuilder::new(dimension as u64, Distance::Cosine)),
            )
            .await
            .map_err(|e| Error::VectorStore(format!("Failed to create collection: {}", e)))?;

        info!(collection = %collection_name, dimension, "Created Qdrant collection");

        Ok(())
    }

    /// Delete a collection.
    pub async fn delete_collection(&self, project_slug: &str) -> Result<()> {
        let collection_name = self.collection_name(project_slug);

        let exists = self
            .inner
            .client
            .collection_exists(&collection_name)
            .await
            .map_err(|e| Error::VectorStore(format!("Failed to check collection: {}", e)))?;

        if !exists {
            return Ok(());
        }

        self.inner
            .client
            .delete_collection(&collection_name)
            .await
            .map_err(|e| Error::VectorStore(format!("Failed to delete collection: {}", e)))?;

        info!(collection = %collection_name, "Deleted Qdrant collection");

        Ok(())
    }

    /// Upsert a single point.
    pub async fn upsert(
        &self,
        project_slug: &str,
        id: &str,
        vector: Vec<f32>,
        payload: HashMap<String, Value>,
    ) -> Result<()> {
        self.upsert_batch(project_slug, vec![(id.to_string(), vector, payload)])
            .await
    }

    /// Upsert multiple points in a batch.
    pub async fn upsert_batch(
        &self,
        project_slug: &str,
        points: Vec<(String, Vec<f32>, HashMap<String, Value>)>,
    ) -> Result<()> {
        if points.is_empty() {
            return Ok(());
        }

        let collection_name = self.collection_name(project_slug);

        let qdrant_points: Vec<PointStruct> = points
            .into_iter()
            .map(|(id, vector, payload)| {
                let qdrant_payload: HashMap<String, QdrantValue> = payload
                    .into_iter()
                    .filter_map(|(k, v)| json_to_qdrant_value(v).map(|qv| (k, qv)))
                    .collect();

                PointStruct::new(id, vector, qdrant_payload)
            })
            .collect();

        let count = qdrant_points.len();

        self.inner
            .client
            .upsert_points(UpsertPointsBuilder::new(&collection_name, qdrant_points))
            .await
            .map_err(|e| Error::VectorStore(format!("Failed to upsert points: {}", e)))?;

        debug!(collection = %collection_name, count, "Upserted points");

        Ok(())
    }

    /// Search for similar vectors.
    pub async fn search(
        &self,
        project_slug: &str,
        vector: Vec<f32>,
        limit: usize,
        filter: Option<SearchFilter>,
    ) -> Result<Vec<VectorSearchResult>> {
        let collection_name = self.collection_name(project_slug);

        let mut builder =
            SearchPointsBuilder::new(&collection_name, vector, limit as u64).with_payload(true);

        if let Some(f) = filter {
            builder = builder.filter(f.to_qdrant_filter());
        }

        let response = self
            .inner
            .client
            .search_points(builder)
            .await
            .map_err(|e| Error::VectorStore(format!("Search failed: {}", e)))?;

        let results = response
            .result
            .into_iter()
            .map(scored_point_to_result)
            .collect();

        Ok(results)
    }

    /// Delete a point by ID.
    pub async fn delete(&self, project_slug: &str, id: &str) -> Result<()> {
        self.delete_batch(project_slug, vec![id.to_string()]).await
    }

    /// Delete multiple points by ID.
    pub async fn delete_batch(&self, project_slug: &str, ids: Vec<String>) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }

        let collection_name = self.collection_name(project_slug);

        let point_ids: Vec<PointId> = ids.into_iter().map(PointId::from).collect();

        self.inner
            .client
            .delete_points(DeletePointsBuilder::new(&collection_name).points(point_ids))
            .await
            .map_err(|e| Error::VectorStore(format!("Failed to delete points: {}", e)))?;

        Ok(())
    }

    /// Delete points matching a filter.
    /// Note: Uses scroll + delete batch since DeletePointsBuilder doesn't support filter directly.
    pub async fn delete_by_filter(&self, project_slug: &str, filter: SearchFilter) -> Result<()> {
        // Scroll to find all matching points, then delete by ID
        let (results, _) = self.scroll(project_slug, 1000, None, Some(filter)).await?;

        if results.is_empty() {
            return Ok(());
        }

        let ids: Vec<String> = results.into_iter().map(|r| r.id).collect();
        self.delete_batch(project_slug, ids).await
    }

    /// Get collection info.
    pub async fn collection_info(&self, project_slug: &str) -> Result<CollectionInfo> {
        let collection_name = self.collection_name(project_slug);

        let exists = self
            .inner
            .client
            .collection_exists(&collection_name)
            .await
            .map_err(|e| Error::VectorStore(format!("Failed to check collection: {}", e)))?;

        if !exists {
            return Ok(CollectionInfo {
                name: collection_name,
                exists: false,
                points_count: 0,
                dimension: 0,
            });
        }

        let info = self
            .inner
            .client
            .collection_info(&collection_name)
            .await
            .map_err(|e| Error::VectorStore(format!("Failed to get collection info: {}", e)))?;

        let dim = info
            .result
            .as_ref()
            .and_then(|r| r.config.as_ref())
            .and_then(|c| c.params.as_ref())
            .and_then(|p| p.vectors_config.as_ref())
            .and_then(|vc| match vc.config.as_ref() {
                Some(qdrant_client::qdrant::vectors_config::Config::Params(params)) => {
                    Some(params.size as usize)
                }
                _ => None,
            })
            .unwrap_or(0);

        Ok(CollectionInfo {
            name: collection_name,
            exists: true,
            points_count: info
                .result
                .map(|r| r.points_count.unwrap_or(0))
                .unwrap_or(0),
            dimension: dim,
        })
    }

    /// Scroll through all points in a collection.
    pub async fn scroll(
        &self,
        project_slug: &str,
        limit: usize,
        offset: Option<String>,
        filter: Option<SearchFilter>,
    ) -> Result<(Vec<VectorSearchResult>, Option<String>)> {
        let collection_name = self.collection_name(project_slug);

        let mut builder = ScrollPointsBuilder::new(&collection_name)
            .limit(limit as u32)
            .with_payload(true);

        if let Some(off) = offset {
            builder = builder.offset(PointId::from(off));
        }

        if let Some(f) = filter {
            builder = builder.filter(f.to_qdrant_filter());
        }

        let response = self
            .inner
            .client
            .scroll(builder)
            .await
            .map_err(|e| Error::VectorStore(format!("Scroll failed: {}", e)))?;

        let results: Vec<VectorSearchResult> = response
            .result
            .into_iter()
            .map(|point| {
                let id = match point.id {
                    Some(PointId {
                        point_id_options:
                            Some(qdrant_client::qdrant::point_id::PointIdOptions::Uuid(uuid)),
                    }) => uuid,
                    Some(PointId {
                        point_id_options:
                            Some(qdrant_client::qdrant::point_id::PointIdOptions::Num(num)),
                    }) => num.to_string(),
                    _ => String::new(),
                };

                let payload = point
                    .payload
                    .into_iter()
                    .filter_map(|(k, v)| qdrant_value_to_json(v).map(|jv| (k, jv)))
                    .collect();

                VectorSearchResult {
                    id,
                    score: 1.0,
                    payload,
                }
            })
            .collect();

        let next_offset = response
            .next_page_offset
            .and_then(|id| match id.point_id_options {
                Some(qdrant_client::qdrant::point_id::PointIdOptions::Uuid(uuid)) => Some(uuid),
                Some(qdrant_client::qdrant::point_id::PointIdOptions::Num(num)) => {
                    Some(num.to_string())
                }
                _ => None,
            });

        Ok((results, next_offset))
    }
}

/// Collection information
#[derive(Debug, Clone)]
pub struct CollectionInfo {
    pub name: String,
    pub exists: bool,
    pub points_count: u64,
    pub dimension: usize,
}

/// Search filter for Qdrant queries
#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    pub memory_type: Option<String>,
    pub author: Option<String>,
    pub file_path: Option<String>,
    pub project_id: Option<String>,
}

impl SearchFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_type(mut self, memory_type: &str) -> Self {
        self.memory_type = Some(memory_type.to_string());
        self
    }

    pub fn with_author(mut self, author: &str) -> Self {
        self.author = Some(author.to_string());
        self
    }

    pub fn with_file_path(mut self, path: &str) -> Self {
        self.file_path = Some(path.to_string());
        self
    }

    pub fn with_project_id(mut self, project_id: &str) -> Self {
        self.project_id = Some(project_id.to_string());
        self
    }

    fn to_qdrant_filter(&self) -> Filter {
        let mut conditions = Vec::new();

        if let Some(ref t) = self.memory_type {
            conditions.push(make_match_condition(KEY_TYPE, t));
        }

        if let Some(ref a) = self.author {
            conditions.push(make_match_condition(KEY_AUTHOR, a));
        }

        if let Some(ref p) = self.file_path {
            conditions.push(make_match_condition(KEY_FILE_PATH, p));
        }

        if let Some(ref pid) = self.project_id {
            conditions.push(make_match_condition(KEY_PROJECT_ID, pid));
        }

        Filter {
            must: conditions,
            ..Default::default()
        }
    }
}

/// Create a match condition for a field
fn make_match_condition(key: &str, value: &str) -> Condition {
    Condition {
        condition_one_of: Some(ConditionOneOf::Field(FieldCondition {
            key: key.to_string(),
            r#match: Some(Match {
                match_value: Some(MatchValue::Keyword(value.to_string())),
            }),
            ..Default::default()
        })),
    }
}

/// Convert JSON value to Qdrant value
fn json_to_qdrant_value(value: Value) -> Option<QdrantValue> {
    match value {
        Value::Null => None,
        Value::Bool(b) => Some(QdrantValue::from(b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(QdrantValue::from(i))
            } else if let Some(f) = n.as_f64() {
                Some(QdrantValue::from(f))
            } else {
                None
            }
        }
        Value::String(s) => Some(QdrantValue::from(s)),
        Value::Array(arr) => {
            let values: Vec<QdrantValue> =
                arr.into_iter().filter_map(json_to_qdrant_value).collect();
            if values.is_empty() {
                None
            } else {
                Some(QdrantValue::from(values))
            }
        }
        Value::Object(_) => {
            // Qdrant doesn't support nested objects directly, serialize to string
            Some(QdrantValue::from(value.to_string()))
        }
    }
}

/// Convert Qdrant value to JSON value
fn qdrant_value_to_json(value: QdrantValue) -> Option<Value> {
    use qdrant_client::qdrant::value::Kind;

    match value.kind {
        Some(Kind::NullValue(_)) => Some(Value::Null),
        Some(Kind::BoolValue(b)) => Some(Value::Bool(b)),
        Some(Kind::IntegerValue(i)) => Some(Value::Number(i.into())),
        Some(Kind::DoubleValue(d)) => serde_json::Number::from_f64(d).map(Value::Number),
        Some(Kind::StringValue(s)) => Some(Value::String(s)),
        Some(Kind::ListValue(list)) => {
            let values: Vec<Value> = list
                .values
                .into_iter()
                .filter_map(qdrant_value_to_json)
                .collect();
            Some(Value::Array(values))
        }
        Some(Kind::StructValue(obj)) => {
            let map: serde_json::Map<String, Value> = obj
                .fields
                .into_iter()
                .filter_map(|(k, v)| qdrant_value_to_json(v).map(|jv| (k, jv)))
                .collect();
            Some(Value::Object(map))
        }
        None => None,
    }
}

/// Convert scored point to search result
fn scored_point_to_result(point: ScoredPoint) -> VectorSearchResult {
    let id = match point.id {
        Some(PointId {
            point_id_options: Some(qdrant_client::qdrant::point_id::PointIdOptions::Uuid(uuid)),
        }) => uuid,
        Some(PointId {
            point_id_options: Some(qdrant_client::qdrant::point_id::PointIdOptions::Num(num)),
        }) => num.to_string(),
        _ => String::new(),
    };

    let payload = point
        .payload
        .into_iter()
        .filter_map(|(k, v)| qdrant_value_to_json(v).map(|jv| (k, jv)))
        .collect();

    VectorSearchResult {
        id,
        score: point.score,
        payload,
    }
}
