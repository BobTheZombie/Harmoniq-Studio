use std::cell::Cell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

use harmoniq_engine::project::{
    load_project, save_autosave, save_project, LoadOptions, MediaAsset, MediaChecksum,
    ProjectDocument, ProjectMediaEntryV1, ProjectMetadata, ProjectV1, SaveOptions,
};
use harmoniq_engine::ProjectLoadError;
use tempfile::TempDir;

fn sample_metadata() -> ProjectMetadata {
    ProjectMetadata::new("Example", 48_000.0, 512, 2, 120.0)
}

fn create_media_asset(base: &TempDir) -> (MediaAsset, Vec<u8>, PathBuf) {
    let relative_path = PathBuf::from("audio/sample.wav");
    let data = (0..128u8).collect::<Vec<_>>();
    let mut asset = MediaAsset::new("clip1", &relative_path, data.clone());
    let absolute = base.path().join(&relative_path);
    if let Some(parent) = absolute.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&absolute, &data).unwrap();
    asset.resolved_path = Some(absolute.clone());
    (asset, data, absolute)
}

#[test]
fn project_roundtrip_persists_media_chunks() {
    let dir = TempDir::new().unwrap();
    let (asset, data, resolved) = create_media_asset(&dir);
    let metadata = sample_metadata();
    let document = ProjectDocument::new(metadata.clone(), vec![asset.clone()]);
    let project_path = dir.path().join("session.hsq");

    let report = save_project(&project_path, &document, SaveOptions::default()).unwrap();
    assert!(!report.autosave);
    assert!(project_path.exists());

    let load = load_project(&project_path, LoadOptions::default()).unwrap();
    assert!(!load.recovered_from_autosave);
    assert_eq!(load.document.metadata.name, metadata.name);
    assert_eq!(load.document.media.len(), 1);
    let loaded_asset = &load.document.media[0];
    assert_eq!(loaded_asset.data, data);
    assert_eq!(loaded_asset.relative_path, asset.relative_path);
    assert_eq!(loaded_asset.resolved_path.as_ref().unwrap(), &resolved);
}

#[test]
fn missing_media_can_be_relinked_with_embedded_data() {
    let dir = TempDir::new().unwrap();
    let (asset, data, resolved) = create_media_asset(&dir);
    let document = ProjectDocument::new(sample_metadata(), vec![asset]);
    let project_path = dir.path().join("session.hsq");
    save_project(&project_path, &document, SaveOptions::default()).unwrap();
    fs::remove_file(resolved).unwrap();

    let error = load_project(&project_path, LoadOptions::default()).unwrap_err();
    match error {
        ProjectLoadError::MissingMedia { .. } => {}
        other => panic!("expected missing media error, got {other:?}"),
    }

    let recovered_flag = Rc::new(Cell::new(false));
    let flag = recovered_flag.clone();
    let base = dir.path().to_path_buf();
    let options = LoadOptions {
        prefer_autosave: true,
        relinker: Some(Box::new(move |request| {
            flag.set(true);
            assert_eq!(request.data, data.as_slice());
            let new_path = base.join("audio/relinked.wav");
            if let Some(parent) = new_path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&new_path, request.data).unwrap();
            Some(new_path)
        })),
    };
    let load = load_project(&project_path, options).unwrap();

    assert!(recovered_flag.get());
    let asset = &load.document.media[0];
    assert_eq!(asset.relative_path, PathBuf::from("audio/relinked.wav"));
    assert!(asset.resolved_path.as_ref().unwrap().exists());
}

#[test]
fn autosave_is_preferred_when_newer() {
    let dir = TempDir::new().unwrap();
    let (asset, _, _) = create_media_asset(&dir);
    let document = ProjectDocument::new(sample_metadata(), vec![asset.clone()]);
    let project_path = dir.path().join("session.hsq");
    save_project(&project_path, &document, SaveOptions::default()).unwrap();

    let mut autosave_doc = document.clone();
    autosave_doc.metadata.name = "Recovered".to_string();
    autosave_doc.media[0].data = vec![9; 64];
    autosave_doc.media[0].checksum = MediaChecksum::from_data(&autosave_doc.media[0].data);
    save_autosave(&project_path, &autosave_doc).unwrap();

    let load = load_project(&project_path, LoadOptions::default()).unwrap();
    assert!(load.recovered_from_autosave);
    assert_eq!(load.document.metadata.name, "Recovered");

    let no_autosave = load_project(
        &project_path,
        LoadOptions {
            prefer_autosave: false,
            relinker: None,
        },
    )
    .unwrap();
    assert!(!no_autosave.recovered_from_autosave);
    assert_eq!(no_autosave.document.metadata.name, document.metadata.name);
}

#[test]
fn migration_from_v1_creates_v2_document() {
    let dir = TempDir::new().unwrap();
    let audio_path = dir.path().join("samples/clip.wav");
    if let Some(parent) = audio_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let audio_data = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
    fs::write(&audio_path, &audio_data).unwrap();

    let project_v1 = ProjectV1 {
        name: "Legacy".into(),
        sample_rate: 44_100.0,
        block_size: 256,
        channels: 2,
        duration_seconds: 60.0,
        media: vec![ProjectMediaEntryV1 {
            path: PathBuf::from("samples/clip.wav"),
        }],
    };

    let project_path = dir.path().join("legacy.json");
    fs::write(
        &project_path,
        serde_json::to_vec_pretty(&project_v1).unwrap(),
    )
    .unwrap();

    let report = load_project(&project_path, LoadOptions::default()).unwrap();
    assert_eq!(report.document.version, harmoniq_engine::PROJECT_VERSION);
    assert_eq!(report.document.metadata.name, "Legacy");
    assert_eq!(report.document.media.len(), 1);
    let asset = &report.document.media[0];
    assert_eq!(asset.relative_path, PathBuf::from("samples/clip.wav"));
    assert_eq!(asset.data, audio_data);

    let migrated_path = dir.path().join("legacy_migrated.hsq");
    save_project(&migrated_path, &report.document, SaveOptions::default()).unwrap();
    assert!(migrated_path.exists());
}
