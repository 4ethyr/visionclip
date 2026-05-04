pub mod catalog;
pub mod config;
pub mod crawler;
pub mod extractors;
pub mod index;
pub mod jobs;
pub mod metrics;
pub mod query;
pub mod schema;
pub mod security;
pub mod service;
pub mod watcher;

pub use catalog::{CatalogStats, SearchAudit, SearchCatalog, SearchHitRecord};
pub use config::{RankingConfig, SearchRuntimeConfig};
pub use crawler::{crawl_roots, CrawlSummary};
pub use security::{PathSensitivity, SecurityPolicy};
pub use service::{LocalSearchMode, LocalSearchRequest, SearchControlReport, SearchService};
