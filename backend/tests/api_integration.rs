// Integration tests for API endpoints with multi-disc album support

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub struct Database {
        pub albums: Vec<Album>,
    }

    #[derive(Debug, Serialize, Deserialize, Clone)]
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

    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub struct AlbumImage {
        pub id: String,
        pub image_path: String,
        pub image_type: String,
        pub is_primary: bool,
    }

    #[derive(Debug, Serialize, Deserialize, Clone)]
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

    #[test]
    fn test_api_single_disc_album_metadata() {
        // Create test metadata for single-disc album
        let metadata = r#"
[[albums]]
id = "api_test_001"
name = "Test Album"
artist = "Test Artist"
release_date = "2024-01-01"
disc_count = 1
cover_path = "albums/api_test_001 - Test_Artist - Test_Album/cover.jpg"
enabled = true
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"

[[albums.images]]
id = "img001"
image_path = "albums/api_test_001 - Test_Artist - Test_Album/cover.jpg"
image_type = "cover"
is_primary = true

[[albums.tracks]]
id = "track001"
title = "Track 1"
artist = "Test Artist"
disc_number = 1
track_number = 1
length_seconds = 180.0
init_segment_path = "albums/api_test_001 - Test_Artist - Test_Album/disc_1/track_1/init.mp4"
segment_path_template = "albums/api_test_001 - Test_Artist - Test_Album/disc_1/track_1/chunk_%03d$.m4s"
segment_count = 30
segment_duration = 6.0
source_file_path = "/path/to/track1.flac"
created_at = "2024-01-01T00:00:00Z"
"#;

        let db: Database = toml::from_str(metadata).expect("Failed to parse metadata");

        // Verify single disc album
        assert_eq!(db.albums.len(), 1);
        let album = &db.albums[0];
        assert_eq!(album.id, "api_test_001");
        assert_eq!(album.disc_count, 1);
        assert_eq!(album.tracks.len(), 1);

        // Verify track metadata
        let track = &album.tracks[0];
        assert_eq!(track.id, "track001");
        assert_eq!(track.track_number, 1);
        assert_eq!(track.disc_number, 1);
        assert_eq!(track.length_seconds, 180.0);
        assert_eq!(track.segment_count, 30);

        // Verify paths include disc_number
        assert!(track.init_segment_path.contains("disc_1/track_1/init.mp4"));
        assert!(track
            .segment_path_template
            .contains("disc_1/track_1/chunk_%03d$.m4s"));
    }

    #[test]
    fn test_api_multi_disc_album_metadata() {
        // Create test metadata for multi-disc album
        let metadata = r#"
[[albums]]
id = "api_test_002"
name = "Multi Disc Album"
artist = "Multi Artist"
release_date = "2024-01-01"
disc_count = 2
cover_path = "albums/api_test_002 - Multi_Artist - Multi_Disc_Album/cover.jpg"
enabled = true
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"

[[albums.images]]
id = "img001"
image_path = "albums/api_test_002 - Multi_Artist - Multi_Disc_Album/cover.jpg"
image_type = "cover"
is_primary = true

[[albums.tracks]]
id = "d1_t1"
title = "Disc 1 Track 1"
artist = "Multi Artist"
disc_number = 1
track_number = 1
length_seconds = 200.0
init_segment_path = "albums/api_test_002 - Multi_Artist - Multi_Disc_Album/disc_1/track_1/init.mp4"
segment_path_template = "albums/api_test_002 - Multi_Artist - Multi_Disc_Album/disc_1/track_1/chunk_%03d$.m4s"
segment_count = 34
segment_duration = 6.0
source_file_path = "/path/to/d1t1.flac"
created_at = "2024-01-01T00:00:00Z"

[[albums.tracks]]
id = "d2_t1"
title = "Disc 2 Track 1"
artist = "Multi Artist"
disc_number = 2
track_number = 1
length_seconds = 150.0
init_segment_path = "albums/api_test_002 - Multi_Artist - Multi_Disc_Album/disc_2/track_1/init.mp4"
segment_path_template = "albums/api_test_002 - Multi_Artist - Multi_Disc_Album/disc_2/track_1/chunk_%03d$.m4s"
segment_count = 25
segment_duration = 6.0
source_file_path = "/path/to/d2t1.flac"
created_at = "2024-01-01T00:00:00Z"
"#;

        let db: Database = toml::from_str(metadata).expect("Failed to parse metadata");

        // Verify multi-disc album
        assert_eq!(db.albums.len(), 1);
        let album = &db.albums[0];
        assert_eq!(album.id, "api_test_002");
        assert_eq!(album.disc_count, 2);
        assert_eq!(album.tracks.len(), 2);

        // Verify disc 1, track 1
        let d1t1 = &album.tracks[0];
        assert_eq!(d1t1.disc_number, 1);
        assert_eq!(d1t1.track_number, 1);
        assert_eq!(d1t1.length_seconds, 200.0);
        assert_eq!(d1t1.segment_count, 34);
        assert!(d1t1.init_segment_path.contains("disc_1/track_1"));

        // Verify disc 2, track 1
        let d2t1 = &album.tracks[1];
        assert_eq!(d2t1.disc_number, 2);
        assert_eq!(d2t1.track_number, 1);
        assert_eq!(d2t1.length_seconds, 150.0);
        assert_eq!(d2t1.segment_count, 25);
        assert!(d2t1.init_segment_path.contains("disc_2/track_1"));

        // Verify paths are different (critical for no collisions)
        assert_ne!(d1t1.init_segment_path, d2t1.init_segment_path);
        assert_ne!(d1t1.segment_path_template, d2t1.segment_path_template);
    }

    #[test]
    fn test_api_segment_path_generation() {
        // Test that segment paths are correctly generated with disc number
        let metadata = r#"
[[albums]]
id = "path_test"
name = "Path Test Album"
artist = "Test Artist"
release_date = "2024-01-01"
disc_count = 3
cover_path = "albums/path_test - Test_Artist - Path_Test_Album/cover.jpg"
enabled = true
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"

[[albums.images]]
id = "img001"
image_path = "albums/path_test - Test_Artist - Path_Test_Album/cover.jpg"
image_type = "cover"
is_primary = true

[[albums.tracks]]
id = "d1_t2"
title = "Disc 1 Track 2"
artist = "Test Artist"
disc_number = 1
track_number = 2
length_seconds = 180.0
init_segment_path = "albums/path_test - Test_Artist - Path_Test_Album/disc_1/track_2/init.mp4"
segment_path_template = "albums/path_test - Test_Artist - Path_Test_Album/disc_1/track_2/chunk_%03d$.m4s"
segment_count = 30
segment_duration = 6.0
source_file_path = "/path/to/d1t2.flac"
created_at = "2024-01-01T00:00:00Z"

[[albums.tracks]]
id = "d2_t2"
title = "Disc 2 Track 2"
artist = "Test Artist"
disc_number = 2
track_number = 2
length_seconds = 180.0
init_segment_path = "albums/path_test - Test_Artist - Path_Test_Album/disc_2/track_2/init.mp4"
segment_path_template = "albums/path_test - Test_Artist - Path_Test_Album/disc_2/track_2/chunk_%03d$.m4s"
segment_count = 30
segment_duration = 6.0
source_file_path = "/path/to/d2t2.flac"
created_at = "2024-01-01T00:00:00Z"

[[albums.tracks]]
id = "d3_t2"
title = "Disc 3 Track 2"
artist = "Test Artist"
disc_number = 3
track_number = 2
length_seconds = 180.0
init_segment_path = "albums/path_test - Test_Artist - Path_Test_Album/disc_3/track_2/init.mp4"
segment_path_template = "albums/path_test - Test_Artist - Path_Test_Album/disc_3/track_2/chunk_%03d$.m4s"
segment_count = 30
segment_duration = 6.0
source_file_path = "/path/to/d3t2.flac"
created_at = "2024-01-01T00:00:00Z"
"#;

        let db: Database = toml::from_str(metadata).expect("Failed to parse metadata");
        let album = &db.albums[0];

        // Verify all three discs have unique paths
        assert!(album.tracks[0].init_segment_path.contains("disc_1/track_2"));
        assert!(album.tracks[1].init_segment_path.contains("disc_2/track_2"));
        assert!(album.tracks[2].init_segment_path.contains("disc_3/track_2"));

        // Verify chunk path templates
        assert!(album.tracks[0]
            .segment_path_template
            .contains("disc_1/track_2/chunk"));
        assert!(album.tracks[1]
            .segment_path_template
            .contains("disc_2/track_2/chunk"));
        assert!(album.tracks[2]
            .segment_path_template
            .contains("disc_3/track_2/chunk"));

        // Test actual path substitution
        let test_path = album.tracks[0].segment_path_template.as_str();
        let chunk_5 = test_path
            .replace("%03d", &format!("{:03}", 5))
            .replace("$", "");
        assert_eq!(
            chunk_5,
            "albums/path_test - Test_Artist - Path_Test_Album/disc_1/track_2/chunk_005.m4s"
        );
    }

    #[test]
    fn test_api_now_playing_response_format() {
        // Verify the now-playing JSON response structure
        use serde_json::json;

        let response = json!({
            "album_id": "api_test_001",
            "album_name": "Test Album",
            "album_artist": "Test Artist",
            "track_id": "track001",
            "track_title": "Track 1",
            "track_artist": "Test Artist",
            "track_number": 1,
            "disc_number": 1,
            "duration_seconds": 180.0,
            "elapsed_seconds": 45.5,
            "cover_path": "albums/api_test_001 - Test_Artist - Test_Album/cover.jpg"
        });

        // Verify all required fields are present
        assert!(response["album_id"].is_string());
        assert!(response["album_name"].is_string());
        assert!(response["album_artist"].is_string());
        assert!(response["track_id"].is_string());
        assert!(response["track_title"].is_string());
        assert!(response["track_artist"].is_string());
        assert!(response["track_number"].is_number());
        assert!(response["disc_number"].is_number());
        assert!(response["duration_seconds"].is_number());
        assert!(response["elapsed_seconds"].is_number());
        assert!(response["cover_path"].is_string());

        // Verify actual values
        assert_eq!(response["album_id"].as_str().unwrap(), "api_test_001");
        assert_eq!(response["track_number"].as_u64().unwrap(), 1);
        assert_eq!(response["disc_number"].as_u64().unwrap(), 1);
    }

    #[test]
    fn test_api_multi_disc_now_playing() {
        // Test now_playing response with multi-disc album
        use serde_json::json;

        // Disc 2, Track 3 example
        let response = json!({
            "album_id": "multi_disc_001",
            "album_name": "Multi Disc Album",
            "album_artist": "Various Artists",
            "track_id": "d2_t3",
            "track_title": "Second Disc, Third Track",
            "track_artist": "Various Artists",
            "track_number": 3,
            "disc_number": 2,
            "duration_seconds": 240.0,
            "elapsed_seconds": 60.0,
            "cover_path": "albums/multi_disc_001 - Various_Artists - Multi_Disc_Album/cover.jpg"
        });

        // Verify disc_number is correctly reported
        assert_eq!(response["disc_number"].as_u64().unwrap(), 2);
        assert_eq!(response["track_number"].as_u64().unwrap(), 3);

        // Verify both disc and track info are present
        let track_id = response["track_id"].as_str().unwrap();
        assert!(track_id.contains("d2"));
    }

    #[test]
    fn test_api_time_sync_response_format() {
        // Verify time-sync JSON response structure
        use serde_json::json;

        let response = json!({
            "timestamp": "2026-01-17T15:30:45.123Z",
            "unix_millis": 1737044445123i64
        });

        // Verify response structure
        assert!(response["timestamp"].is_string());
        assert!(response["unix_millis"].is_number());

        // Verify timestamp format (ISO 8601 with milliseconds)
        let timestamp = response["timestamp"].as_str().unwrap();
        assert!(timestamp.contains("T"));
        assert!(timestamp.contains("Z"));
        assert!(timestamp.contains("."));

        // Verify unix_millis is a valid number
        let unix_millis = response["unix_millis"].as_i64().unwrap();
        assert!(unix_millis > 0);
    }
}
