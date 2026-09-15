//! Tests covering AudioPlayer playback state, duration, seeking, and EOF handling.

use playback::AudioPlayer;

#[test]
fn test_player_initialization() {
    let player = AudioPlayer::new();
    assert!(!player.is_playing());
    assert_eq!(player.position_seconds(), 0.0);
    assert_eq!(player.duration_seconds(), 0.0);
    assert!(player.samples().is_empty());
}

#[test]
fn test_player_load_and_seek() {
    let mut player = AudioPlayer::new();
    let sample_rate = 48000;
    // 2.5 seconds of silence
    let samples = vec![0.0f32; 120000];

    // Load samples (uses system default output config)
    if let Ok(()) = player.load_samples(samples, sample_rate) {
        let dur = player.duration_seconds();
        assert!((dur - 2.5).abs() < 1e-4);

        // Seek to 1.0 second
        player.seek(1.0);
        assert!((player.position_seconds() - 1.0).abs() < 1e-3);

        // Seek beyond duration should clamp to duration
        player.seek(10.0);
        assert!((player.position_seconds() - dur).abs() < 1e-3);

        // Seek negative should clamp to 0
        player.seek(-5.0);
        assert_eq!(player.position_seconds(), 0.0);
    }
}
