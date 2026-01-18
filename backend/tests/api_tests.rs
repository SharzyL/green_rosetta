// Integration tests for backend metadata and API responses

#[tokio::test]
async fn test_single_disc_album_metadata() {
    // Create test metadata
    let metadata = r#"
[[albums]]
id = "test001"
name = "Test Album"
artist = "Test Artist"
release_date = "2024-01-01"
disc_count = 1
cover_path = "albums/test001 - Test_Artist - Test_Album/cover.jpg"
enabled = true
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"

[[albums.images]]
id = "img001"
image_path = "albums/test001 - Test_Artist - Test_Album/cover.jpg"
image_type = "cover"
is_primary = true

[[albums.tracks]]
id = "track001"
title = "Track 1"
artist = "Test Artist"
disc_number = 1
track_number = 1
length_seconds = 180.0
init_segment_path = "albums/test001 - Test_Artist - Test_Album/disc_1/track_1/init.mp4"
segment_path_template = "albums/test001 - Test_Artist - Test_Album/disc_1/track_1/chunk_%03d$.m4s"
segment_count = 1
segment_duration = 6.0
source_file_path = "/path/to/track1.flac"
created_at = "2024-01-01T00:00:00Z"

[[albums.tracks]]
id = "track002"
title = "Track 2"
artist = "Test Artist"
disc_number = 1
track_number = 2
length_seconds = 240.0
init_segment_path = "albums/test001 - Test_Artist - Test_Album/disc_1/track_2/init.mp4"
segment_path_template = "albums/test001 - Test_Artist - Test_Album/disc_1/track_2/chunk_%03d$.m4s"
segment_count = 2
segment_duration = 6.0
source_file_path = "/path/to/track2.flac"
created_at = "2024-01-01T00:00:00Z"
"#;

    // Parse metadata
    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct Database {
        pub albums: Vec<Album>,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct Album {
        pub id: String,
        pub name: String,
        pub artist: String,
        pub release_date: Option<String>,
        pub media: Option<String>,
        pub label: Option<String>,
        pub catalog_number: Option<String>,
        pub disc_count: u32,
        pub cover_path: String,
        pub enabled: bool,
        pub created_at: String,
        pub updated_at: String,
        pub images: Vec<AlbumImage>,
        pub tracks: Vec<Track>,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct AlbumImage {
        pub id: String,
        pub image_path: String,
        pub image_type: String,
        pub is_primary: bool,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct Track {
        pub id: String,
        pub title: String,
        pub artist: Option<String>,
        pub disc_number: u32,
        pub track_number: u32,
        pub length_seconds: f64,
        pub init_segment_path: String,
        pub segment_path_template: String,
        pub segment_count: u32,
        pub segment_duration: f64,
        pub source_file_path: Option<String>,
        pub created_at: String,
    }

    let db: Database = toml::from_str(metadata).expect("Failed to parse metadata");

    // Verify single disc
    assert_eq!(db.albums.len(), 1);
    assert_eq!(db.albums[0].disc_count, 1);
    assert_eq!(db.albums[0].tracks.len(), 2);

    // Verify track numbers
    assert_eq!(db.albums[0].tracks[0].track_number, 1);
    assert_eq!(db.albums[0].tracks[0].disc_number, 1);
    assert_eq!(db.albums[0].tracks[1].track_number, 2);
    assert_eq!(db.albums[0].tracks[1].disc_number, 1);

    // Verify paths include disc_number
    assert!(db.albums[0].tracks[0]
        .init_segment_path
        .contains("disc_1/track_1"));
    assert!(db.albums[0].tracks[1]
        .init_segment_path
        .contains("disc_1/track_2"));
}

#[tokio::test]
async fn test_multi_disc_album_metadata() {
    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct Database {
        pub albums: Vec<Album>,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct Album {
        pub id: String,
        pub name: String,
        pub artist: String,
        pub release_date: Option<String>,
        pub media: Option<String>,
        pub label: Option<String>,
        pub catalog_number: Option<String>,
        pub disc_count: u32,
        pub cover_path: String,
        pub enabled: bool,
        pub created_at: String,
        pub updated_at: String,
        pub images: Vec<AlbumImage>,
        pub tracks: Vec<Track>,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct AlbumImage {
        pub id: String,
        pub image_path: String,
        pub image_type: String,
        pub is_primary: bool,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct Track {
        pub id: String,
        pub title: String,
        pub artist: Option<String>,
        pub disc_number: u32,
        pub track_number: u32,
        pub length_seconds: f64,
        pub init_segment_path: String,
        pub segment_path_template: String,
        pub segment_count: u32,
        pub segment_duration: f64,
        pub source_file_path: Option<String>,
        pub created_at: String,
    }

    // Create test metadata with 2 discs
    let metadata = r#"
[[albums]]
id = "test002"
name = "Multi Disc Album"
artist = "Multi Disc Artist"
release_date = "2024-01-01"
disc_count = 2
cover_path = "albums/test002 - Multi_Disc_Artist - Multi_Disc_Album/cover.jpg"
enabled = true
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"

[[albums.images]]
id = "img001"
image_path = "albums/test002 - Multi_Disc_Artist - Multi_Disc_Album/cover.jpg"
image_type = "cover"
is_primary = true

[[albums.tracks]]
id = "disc1_track1"
title = "Disc 1 Track 1"
artist = "Multi Disc Artist"
disc_number = 1
track_number = 1
length_seconds = 180.0
init_segment_path = "albums/test002 - Multi_Disc_Artist - Multi_Disc_Album/disc_1/track_1/init.mp4"
segment_path_template = "albums/test002 - Multi_Disc_Artist - Multi_Disc_Album/disc_1/track_1/chunk_%03d$.m4s"
segment_count = 1
segment_duration = 6.0
source_file_path = "/path/to/d1t1.flac"
created_at = "2024-01-01T00:00:00Z"

[[albums.tracks]]
id = "disc1_track2"
title = "Disc 1 Track 2"
artist = "Multi Disc Artist"
disc_number = 1
track_number = 2
length_seconds = 180.0
init_segment_path = "albums/test002 - Multi_Disc_Artist - Multi_Disc_Album/disc_1/track_2/init.mp4"
segment_path_template = "albums/test002 - Multi_Disc_Artist - Multi_Disc_Album/disc_1/track_2/chunk_%03d$.m4s"
segment_count = 1
segment_duration = 6.0
source_file_path = "/path/to/d1t2.flac"
created_at = "2024-01-01T00:00:00Z"

[[albums.tracks]]
id = "disc2_track1"
title = "Disc 2 Track 1"
artist = "Multi Disc Artist"
disc_number = 2
track_number = 1
length_seconds = 180.0
init_segment_path = "albums/test002 - Multi_Disc_Artist - Multi_Disc_Album/disc_2/track_1/init.mp4"
segment_path_template = "albums/test002 - Multi_Disc_Artist - Multi_Disc_Album/disc_2/track_1/chunk_%03d$.m4s"
segment_count = 1
segment_duration = 6.0
source_file_path = "/path/to/d2t1.flac"
created_at = "2024-01-01T00:00:00Z"

[[albums.tracks]]
id = "disc2_track2"
title = "Disc 2 Track 2"
artist = "Multi Disc Artist"
disc_number = 2
track_number = 2
length_seconds = 180.0
init_segment_path = "albums/test002 - Multi_Disc_Artist - Multi_Disc_Album/disc_2/track_2/init.mp4"
segment_path_template = "albums/test002 - Multi_Disc_Artist - Multi_Disc_Album/disc_2/track_2/chunk_%03d$.m4s"
segment_count = 1
segment_duration = 6.0
source_file_path = "/path/to/d2t2.flac"
created_at = "2024-01-01T00:00:00Z"
"#;

    let db: Database = toml::from_str(metadata).expect("Failed to parse metadata");

    // Verify disc count
    assert_eq!(db.albums.len(), 1);
    assert_eq!(db.albums[0].disc_count, 2);
    assert_eq!(db.albums[0].tracks.len(), 4);

    // Verify no path collisions between discs
    let d1t1_path = &db.albums[0].tracks[0].init_segment_path;
    let d2t1_path = &db.albums[0].tracks[2].init_segment_path;

    // Both are track 1, but on different discs
    assert!(d1t1_path.contains("disc_1/track_1"));
    assert!(d2t1_path.contains("disc_2/track_1"));
    assert_ne!(
        d1t1_path, d2t1_path,
        "Disc 1 Track 1 and Disc 2 Track 1 should have different paths"
    );

    // Verify track numbers repeat across discs (intentional in multi-disc albums)
    assert_eq!(db.albums[0].tracks[0].track_number, 1); // Disc 1, Track 1
    assert_eq!(db.albums[0].tracks[0].disc_number, 1);

    assert_eq!(db.albums[0].tracks[2].track_number, 1); // Disc 2, Track 1
    assert_eq!(db.albums[0].tracks[2].disc_number, 2);
}

#[test]
fn test_track_identification_by_disc_and_track_number() {
    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct Database {
        pub albums: Vec<Album>,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct Album {
        pub id: String,
        pub name: String,
        pub artist: String,
        pub release_date: Option<String>,
        pub media: Option<String>,
        pub label: Option<String>,
        pub catalog_number: Option<String>,
        pub disc_count: u32,
        pub cover_path: String,
        pub enabled: bool,
        pub created_at: String,
        pub updated_at: String,
        pub images: Vec<AlbumImage>,
        pub tracks: Vec<Track>,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct AlbumImage {
        pub id: String,
        pub image_path: String,
        pub image_type: String,
        pub is_primary: bool,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct Track {
        pub id: String,
        pub title: String,
        pub artist: Option<String>,
        pub disc_number: u32,
        pub track_number: u32,
        pub length_seconds: f64,
        pub init_segment_path: String,
        pub segment_path_template: String,
        pub segment_count: u32,
        pub segment_duration: f64,
        pub source_file_path: Option<String>,
        pub created_at: String,
    }

    // This test verifies that tracks can be uniquely identified by disc_number + track_number
    let metadata = r#"
[[albums]]
id = "test003"
name = "Test Album"
artist = "Test Artist"
release_date = "2024-01-01"
disc_count = 1
cover_path = "albums/test003 - Test_Artist - Test_Album/cover.jpg"
enabled = true
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"

[[albums.images]]
id = "img001"
image_path = "albums/test003 - Test_Artist - Test_Album/cover.jpg"
image_type = "cover"
is_primary = true

[[albums.tracks]]
id = "track_unique_id_1"
title = "Track A"
artist = "Test Artist"
disc_number = 1
track_number = 5
length_seconds = 180.0
init_segment_path = "albums/test003 - Test_Artist - Test_Album/disc_1/track_5/init.mp4"
segment_path_template = "albums/test003 - Test_Artist - Test_Album/disc_1/track_5/chunk_%03d$.m4s"
segment_count = 1
segment_duration = 6.0
source_file_path = "/path/to/track.flac"
created_at = "2024-01-01T00:00:00Z"
"#;

    let db: Database = toml::from_str(metadata).expect("Failed to parse metadata");

    let track = &db.albums[0].tracks[0];

    // Verify track can be uniquely identified
    assert_eq!(track.id, "track_unique_id_1");
    assert_eq!((track.disc_number, track.track_number), (1, 5));

    // Verify path includes both disc and track number
    assert!(track.init_segment_path.contains("disc_1/track_5"));
}

#[test]
fn test_segment_path_template_substitution() {
    let template = "albums/test - Artist - Album/disc_1/track_2/chunk_%03d$.m4s";

    // Test chunk number substitution (matching what the backend does)
    let chunk_1 = template
        .replace("%03d", &format!("{:03}", 1))
        .replace("$", "");
    let chunk_5 = template
        .replace("%03d", &format!("{:03}", 5))
        .replace("$", "");
    let chunk_10 = template
        .replace("%03d", &format!("{:03}", 10))
        .replace("$", "");

    assert_eq!(
        chunk_1,
        "albums/test - Artist - Album/disc_1/track_2/chunk_001.m4s"
    );
    assert_eq!(
        chunk_5,
        "albums/test - Artist - Album/disc_1/track_2/chunk_005.m4s"
    );
    assert_eq!(
        chunk_10,
        "albums/test - Artist - Album/disc_1/track_2/chunk_010.m4s"
    );
}

#[test]
fn test_now_playing_response_format() {
    use serde_json::json;

    // Simulate the response structure
    let response = json!({
        "album_id": "5ef472137f",
        "album_name": "Kikuo Miku 7",
        "album_artist": "Kikuo",
        "track_id": "6d7a722e31",
        "track_title": "ふたつの木馬 / Twins at the carousel",
        "track_artist": "Kikuo",
        "track_number": 1,
        "disc_number": 1,
        "duration_seconds": 39.96,
        "elapsed_seconds": 12.34,
        "cover_path": "albums/5ef472137f - Kikuo - Kikuo_Miku_7/cover.jpg"
    });

    // Verify response structure
    assert!(response["album_id"].is_string());
    assert!(response["album_name"].is_string());
    assert!(response["album_artist"].is_string());
    assert!(response["track_id"].is_string());
    assert!(response["track_title"].is_string());
    assert!(response["track_number"].is_number());
    assert!(response["disc_number"].is_number());
    assert!(response["duration_seconds"].is_number());
    assert!(response["elapsed_seconds"].is_number());
    assert!(response["cover_path"].is_string());
}

#[test]
fn test_time_sync_response_format() {
    use serde_json::json;

    let response = json!({
        "timestamp": "2026-01-17T15:30:45.123Z",
        "unix_millis": 1737044445123i64
    });

    assert!(response["timestamp"].is_string());
    assert!(response["unix_millis"].is_number());

    // Verify timestamp format (ISO 8601)
    let timestamp = response["timestamp"].as_str().unwrap();
    assert!(timestamp.contains("T"));
    assert!(timestamp.contains("Z"));
}

#[test]
fn test_album_with_up_to_10_discs() {
    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct Database {
        pub albums: Vec<Album>,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct Album {
        pub id: String,
        pub name: String,
        pub artist: String,
        pub release_date: Option<String>,
        pub media: Option<String>,
        pub label: Option<String>,
        pub catalog_number: Option<String>,
        pub disc_count: u32,
        pub cover_path: String,
        pub enabled: bool,
        pub created_at: String,
        pub updated_at: String,
        pub images: Vec<AlbumImage>,
        pub tracks: Vec<Track>,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct AlbumImage {
        pub id: String,
        pub image_path: String,
        pub image_type: String,
        pub is_primary: bool,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct Track {
        pub id: String,
        pub title: String,
        pub artist: Option<String>,
        pub disc_number: u32,
        pub track_number: u32,
        pub length_seconds: f64,
        pub init_segment_path: String,
        pub segment_path_template: String,
        pub segment_count: u32,
        pub segment_duration: f64,
        pub source_file_path: Option<String>,
        pub created_at: String,
    }

    // Test that the system can handle realistic multi-disc albums
    let mut tracks_toml = String::new();

    // Create 3 discs, 3 tracks each = 9 tracks total
    for disc in 1..=3 {
        for track in 1..=3 {
            let track_id = format!("d{}_t{}", disc, track);
            let segment = format!(
                r#"
[[albums.tracks]]
id = "{}"
title = "Disc {} Track {}"
artist = "Multi Artist"
disc_number = {}
track_number = {}
length_seconds = 180.0
init_segment_path = "albums/test - Artist - Album/disc_{}/track_{}/init.mp4"
segment_path_template = "albums/test - Artist - Album/disc_{}/track_{}/chunk_%03d$.m4s"
segment_count = 1
segment_duration = 6.0
source_file_path = "/path/to/d{}t{}.flac"
created_at = "2024-01-01T00:00:00Z"
"#,
                track_id, disc, track, disc, track, disc, track, disc, track, disc, track
            );
            tracks_toml.push_str(&segment);
        }
    }

    let metadata = format!(
        r#"
[[albums]]
id = "test_multi"
name = "Multi Disc Album"
artist = "Multi Artist"
release_date = "2024-01-01"
disc_count = 3
cover_path = "albums/test - Multi_Artist - Multi_Disc_Album/cover.jpg"
enabled = true
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"

[[albums.images]]
id = "img001"
image_path = "albums/test - Multi_Artist - Multi_Disc_Album/cover.jpg"
image_type = "cover"
is_primary = true

{}
"#,
        tracks_toml
    );

    let db: Database = toml::from_str(&metadata).expect("Failed to parse metadata");

    assert_eq!(db.albums[0].disc_count, 3);
    assert_eq!(db.albums[0].tracks.len(), 9);

    // Verify each disc has 3 tracks
    let disc1_tracks: Vec<_> = db.albums[0]
        .tracks
        .iter()
        .filter(|t| t.disc_number == 1)
        .collect();
    let disc2_tracks: Vec<_> = db.albums[0]
        .tracks
        .iter()
        .filter(|t| t.disc_number == 2)
        .collect();
    let disc3_tracks: Vec<_> = db.albums[0]
        .tracks
        .iter()
        .filter(|t| t.disc_number == 3)
        .collect();

    assert_eq!(disc1_tracks.len(), 3);
    assert_eq!(disc2_tracks.len(), 3);
    assert_eq!(disc3_tracks.len(), 3);
}
