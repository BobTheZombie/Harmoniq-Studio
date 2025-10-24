pub mod load;
pub mod migrate;
pub mod save;
pub mod schema;

pub use load::{load_project, LoadError, LoadOptions, LoadReport, RelinkRequest};
pub use migrate::MigrationError;
pub use save::{autosave_path, save_autosave, save_project, SaveError, SaveOptions, SaveReport};
pub use schema::{
    MediaAsset, MediaChecksum, MediaChunkDescriptor, ProjectDocument, ProjectMediaEntryV1,
    ProjectMetadata, ProjectV1, ProjectV2, CURRENT_VERSION, MEDIA_CHUNK_SIZE, PROJECT_MAGIC,
};
