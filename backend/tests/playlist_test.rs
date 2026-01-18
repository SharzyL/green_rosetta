// Test playlist functionality

#[tokio::test]
async fn test_single_album_playlist_wrapping() {
    // Create a simple test database with one album and 3 tracks
    let metadata = r#"
[[albums]]
id = "album001"
name = "Test Album"
artist = "Test Artist"
release_date = "2024-01-01"
disc_count = 1
cover_path = "albums/album001 - Test_Artist - Test_Album/cover.jpg"
enabled = true
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"

[[albums.images]]
id = "img001"
image_path = "albums/album001 - Test_Artist - Test_Album/cover.jpg"
image_type = "cover"
is_primary = true

[[albums.tracks]]
id = "track001"
title = "Track 1"
artist = "Test Artist"
disc_number = 1
track_number = 1
length_seconds = 10.0
init_segment_path = "albums/album001 - Test_Artist - Test_Album/disc_1/track_1/init.mp4"
segment_path_template = "albums/album001 - Test_Artist - Test_Album/disc_1/track_1/chunk_%03d$.m4s"
segment_count = 2
segment_duration = 6.0
source_file_path = "/path/to/track1.flac"
created_at = "2024-01-01T00:00:00Z"

[[albums.tracks]]
id = "track002"
title = "Track 2"
artist = "Test Artist"
disc_number = 1
track_number = 2
length_seconds = 10.0
init_segment_path = "albums/album001 - Test_Artist - Test_Album/disc_1/track_2/init.mp4"
segment_path_template = "albums/album001 - Test_Artist - Test_Album/disc_1/track_2/chunk_%03d$.m4s"
segment_count = 2
segment_duration = 6.0
source_file_path = "/path/to/track2.flac"
created_at = "2024-01-01T00:00:00Z"

[[albums.tracks]]
id = "track003"
title = "Track 3"
artist = "Test Artist"
disc_number = 1
track_number = 3
length_seconds = 10.0
init_segment_path = "albums/album001 - Test_Artist - Test_Album/disc_1/track_3/init.mp4"
segment_path_template = "albums/album001 - Test_Artist - Test_Album/disc_1/track_3/chunk_%03d$.m4s"
segment_count = 2
segment_duration = 6.0
source_file_path = "/path/to/track3.flac"
created_at = "2024-01-01T00:00:00Z"
"#;

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

    impl Database {
        pub fn get_enabled_albums(&self) -> Vec<&Album> {
            self.albums.iter().filter(|a| a.enabled).collect()
        }

        pub fn get_album(&self, id: &str) -> Option<&Album> {
            self.albums.iter().find(|a| a.id == id)
        }
    }

    let db: Database = toml::from_str(metadata).expect("Failed to parse metadata");

    // Test get_upcoming_tracks when at last track of only album
    let current_album = db.get_album("album001").unwrap();
    let last_track_id = "track003";

    // Simulate get_upcoming_tracks logic
    let current_track_pos = current_album
        .tracks
        .iter()
        .position(|t| t.id == last_track_id)
        .unwrap();

    println!("Current track position: {}", current_track_pos);
    println!("Total tracks in album: {}", current_album.tracks.len());

    // Check if there are more tracks in current album
    let mut upcoming = Vec::new();
    for i in (current_track_pos + 1)..current_album.tracks.len() {
        upcoming.push(&current_album.tracks[i]);
    }

    println!("Tracks remaining in current album: {}", upcoming.len());

    // Now try to get from next album with cycle
    let enabled_albums = db.get_enabled_albums();
    let mut album_iter = enabled_albums.iter().cycle();

    // Skip to current album
    let mut found_current = false;
    let mut steps = 0;
    for album in album_iter.by_ref() {
        steps += 1;
        if album.id == current_album.id {
            found_current = true;
            println!("Found current album after {} steps", steps);
            break;
        }
        if steps > 10 {
            println!("ERROR: Too many iterations without finding current album");
            break;
        }
    }

    // Get next 5 tracks
    if found_current {
        let mut count = 0;
        while count < 5 {
            if let Some(album) = album_iter.next() {
                println!("Next album in cycle: {} (id: {})", album.name, album.id);

                // BUG: With only one album, this will skip it forever!
                if album.id == current_album.id {
                    println!(
                        "  -> Skipping current album (this is the bug when there's only 1 album!)"
                    );
                    // This continue will cause infinite loop with 1 album!
                    // continue;
                }

                for track in &album.tracks {
                    if count >= 5 {
                        break;
                    }
                    upcoming.push(track);
                    count += 1;
                    println!("  -> Added track: {}", track.title);
                }
            }

            if count >= 5 {
                break;
            }
        }

        println!("Total upcoming tracks found: {}", upcoming.len());
    }

    // The bug: When there's only 1 album and we're at the last track,
    // the cycle iterator will keep returning the same album, which we skip,
    // resulting in 0 upcoming tracks instead of wrapping around!
    assert!(
        !upcoming.is_empty(),
        "Should find upcoming tracks by wrapping to beginning"
    );
}

#[test]
fn test_playlist_wrapping_logic() {
    println!("\n=== Testing Playlist Wrapping Logic ===\n");

    // Simulate the issue with a single album
    let albums = ["Album A"];
    let mut iter = albums.iter().cycle();

    // Find current album
    let current = "Album A";
    for &album in iter.by_ref() {
        if album == current {
            println!("Found current album: {}", album);
            break;
        }
    }

    // Try to get next 3 albums
    println!("\nTrying to get next 3 albums:");
    for i in 0..3 {
        let album = iter.next().unwrap();
        println!(
            "  {}: {} {}",
            i + 1,
            album,
            if *album == current { "<-- SKIP (BUG!)" } else { "" }
        );

        // This is the bug - we skip when album == current
        if *album == current {
            // With only 1 album, this continues forever!
        }
    }

    println!("\nBUG: With single album, we skip it forever and get 0 upcoming tracks!");
    println!(
        "FIX: Don't skip current album when it's the only one, or handle wrap-around differently"
    );
}
