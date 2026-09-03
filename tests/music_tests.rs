use app_web::core::*;
use std::time::Duration;

fn make_engine() -> MusicEngine {
    let configs = vec![
        VoiceConfig {
            waveform: Waveform::Sine,
            base_position: glam::Vec3::new(-1.0, 0.0, 0.0),
            trigger_probability: 0.4,
            octave_offset: -1,
            base_duration: 0.4,
        },
        VoiceConfig {
            waveform: Waveform::Saw,
            base_position: glam::Vec3::new(1.0, 0.0, 0.0),
            trigger_probability: 0.6,
            octave_offset: 0,
            base_duration: 0.25,
        },
        VoiceConfig {
            waveform: Waveform::Triangle,
            base_position: glam::Vec3::new(0.0, 0.0, -1.0),
            trigger_probability: 0.3,
            octave_offset: 1,
            base_duration: 0.6,
        },
    ];
    let params = EngineParams::default();
    MusicEngine::new(configs, params, 42)
}

#[test]
fn midi_to_hz_matches_a4_and_octave() {
    let a4 = midi_to_hz(69.0);
    assert!((a4 - 440.0).abs() < 1e-4);
    let a5 = midi_to_hz(81.0);
    assert!((a5 - 880.0).abs() < 1e-3);
    assert!((a5 / a4 - 2.0).abs() < 1e-4);
}

#[test]
fn midi_to_hz_is_monotonic_over_range() {
    let mut prev = midi_to_hz(20.0);
    for m in 21..=100 {
        let f = midi_to_hz(m as f32);
        assert!(f > prev, "frequency not increasing at midi {m}");
        prev = f;
    }
}

#[test]
fn engine_tick_emits_some_events_over_time() {
    let mut engine = make_engine();
    let mut events = Vec::new();
    let seconds_per_beat = 60.0 / engine.params.bpm.0 as f64;
    for _ in 0..200 {
        engine.tick(Duration::from_secs_f64(seconds_per_beat / 2.0), &mut events);
    }
    assert!(!events.is_empty(), "expected some scheduled events");
    for ev in &events {
        assert!(ev.voice.0 < engine.voices.len());
        assert!(ev.freq.0 > 0.0);
        assert!(ev.velocity >= 0.0 && ev.velocity <= 1.0);
        assert!(ev.duration_sec > 0.0);
    }
}

#[test]
fn engine_toggle_mute_and_solo() {
    let mut engine = make_engine();
    assert!(!engine.voices[1].muted);
    engine.toggle_mute(VoiceIndex(1));
    assert!(engine.voices[1].muted);
    engine.toggle_mute(VoiceIndex(1));
    assert!(!engine.voices[1].muted);

    engine.toggle_solo(VoiceIndex(2));
    for (i, v) in engine.voices.iter().enumerate() {
        if i == 2 {
            assert!(!v.muted);
        } else {
            assert!(v.muted);
        }
    }
    engine.toggle_solo(VoiceIndex(2));
    for v in engine.voices.iter() {
        assert!(!v.muted);
    }
}

#[test]
fn midi_to_hz_octave_doubling_property() {
    // Property: Adding 12 semitones (one octave) should double the frequency
    for midi in 20..100 {
        let freq1 = midi_to_hz(midi as f32);
        let freq2 = midi_to_hz((midi + 12) as f32);
        let ratio = freq2 / freq1;
        assert!(
            (ratio - 2.0).abs() < 1e-6,
            "Octave doubling failed for MIDI {midi}: {freq1} -> {freq2} (ratio: {ratio})"
        );
    }
}

#[test]
fn midi_to_hz_semitone_ratio_property() {
    // Property: Each semitone should multiply frequency by 2^(1/12) ≈ 1.059463
    let semitone_ratio = 2.0_f32.powf(1.0 / 12.0);

    for midi in 30..90 {
        let freq1 = midi_to_hz(midi as f32);
        let freq2 = midi_to_hz((midi + 1) as f32);
        let actual_ratio = freq2 / freq1;
        assert!(
            (actual_ratio - semitone_ratio).abs() < 1e-6,
            "Semitone ratio failed for MIDI {midi} -> {}: expected {semitone_ratio}, got {actual_ratio}",
            midi + 1
        );
    }
}

#[test]
fn midi_to_hz_fractional_values() {
    // Test that fractional MIDI values work correctly (for microtonal support)
    let midi_60 = midi_to_hz(60.0); // C4
    let midi_60_5 = midi_to_hz(60.5); // C4 + 50 cents
    let midi_61 = midi_to_hz(61.0); // C#4

    // 50 cents should be halfway between C4 and C#4 in log frequency space
    let log_60 = midi_60.ln();
    let log_60_5 = midi_60_5.ln();
    let log_61 = midi_61.ln();

    let expected_log_60_5 = (log_60 + log_61) / 2.0;
    assert!(
        (log_60_5 - expected_log_60_5).abs() < 1e-6,
        "Fractional MIDI value 60.5 should be logarithmic midpoint between 60 and 61"
    );
}

#[test]
fn midi_to_hz_extreme_values() {
    // Test extreme but valid MIDI values
    let very_low = midi_to_hz(0.0); // C-1, ~8.18 Hz
    let very_high = midi_to_hz(127.0); // G9, ~12543 Hz

    assert!(
        very_low > 0.0 && very_low < 20.0,
        "MIDI 0 should be audible bass frequency"
    );
    assert!(
        very_high > 10000.0 && very_high < 15000.0,
        "MIDI 127 should be very high frequency"
    );

    // Test that extreme values don't cause overflow/underflow
    assert!(
        very_low.is_finite(),
        "Very low MIDI should produce finite frequency"
    );
    assert!(
        very_high.is_finite(),
        "Very high MIDI should produce finite frequency"
    );
}

#[test]
fn midi_to_hz_negative_values() {
    // Test that negative MIDI values work (sub-audio frequencies)
    let neg_midi = midi_to_hz(-12.0); // One octave below MIDI 0
    let zero_midi = midi_to_hz(0.0);

    let ratio = zero_midi / neg_midi;
    assert!(
        (ratio - 2.0).abs() < 1e-6,
        "MIDI -12 should be exactly one octave below MIDI 0"
    );
}

#[test]
fn midi_to_hz_with_detune_accuracy() {
    // Test that 50¢ detune produces correct frequency ratio
    let midi_60 = midi_to_hz(60.0); // C4
    let midi_60_50cents = midi_to_hz_with_detune(60.0, 50.0); // C4 + 50¢

    // 50 cents should be exactly halfway between C4 and C#4 in log frequency space
    let midi_61 = midi_to_hz(61.0); // C#4
    let expected_ratio = (midi_61 / midi_60).sqrt(); // Geometric mean

    let actual_ratio = midi_60_50cents / midi_60;
    assert!(
        (actual_ratio - expected_ratio).abs() < 1e-6,
        "50¢ detune should produce geometric mean frequency ratio"
    );
}

#[test]
fn midi_to_hz_with_detune_bounds() {
    // Test that detune is properly clamped to ±200¢
    let extreme_high = midi_to_hz_with_detune(60.0, 500.0); // Should clamp to +200¢
    let extreme_low = midi_to_hz_with_detune(60.0, -500.0); // Should clamp to -200¢

    // +200¢ should be exactly 2 semitones up
    let expected_high = midi_to_hz(62.0);
    assert!(
        (extreme_high - expected_high).abs() < 1e-6,
        "Extreme high detune should clamp to +200¢ (2 semitones)"
    );

    // -200¢ should be exactly 2 semitones down
    let expected_low = midi_to_hz(58.0);
    assert!(
        (extreme_low - expected_low).abs() < 1e-6,
        "Extreme low detune should clamp to -200¢ (2 semitones)"
    );
}

#[test]
fn engine_params_detune_default() {
    let params = EngineParams::default();
    assert_eq!(params.detune.0, 0.0, "Default detune should be 0¢");
}

#[test]
fn engine_detune_methods() {
    let mut engine = make_engine();

    engine.set_detune_cents(Cents(50.0));
    assert_eq!(engine.params.detune.0, 50.0, "set_detune_cents should work");

    // Test bounds clamping
    engine.set_detune_cents(Cents(300.0));
    assert_eq!(
        engine.params.detune.0, 200.0,
        "set_detune_cents should clamp to +200¢"
    );

    engine.set_detune_cents(Cents(-300.0));
    assert_eq!(
        engine.params.detune.0, -200.0,
        "set_detune_cents should clamp to -200¢"
    );

    engine.adjust_detune_cents(Cents(25.0));
    assert_eq!(
        engine.params.detune.0, -175.0,
        "adjust_detune_cents should work"
    );

    engine.reset_detune();
    assert_eq!(engine.params.detune.0, 0.0, "reset_detune should work");
}

#[test]
fn engine_bpm_methods_guard_invalid_values() {
    let mut engine = make_engine();

    engine.set_bpm(Bpm(180.0));
    assert_eq!(engine.params.bpm.0, 180.0);

    engine.set_bpm(Bpm(-20.0));
    assert_eq!(engine.params.bpm.0, 1.0, "bpm should clamp to lower bound");

    engine.set_bpm(Bpm(1000.0));
    assert_eq!(
        engine.params.bpm.0, 400.0,
        "bpm should clamp to upper bound"
    );

    engine.set_bpm(Bpm(f32::NAN));
    assert_eq!(
        engine.params.bpm.0, 400.0,
        "non-finite bpm should be ignored"
    );
}

#[test]
fn engine_tick_ignores_invalid_bpm_state() {
    let mut engine = make_engine();
    engine.params.bpm = Bpm(-10.0); // simulate invalid state from external mutation
    let mut events = Vec::new();
    engine.tick(Duration::from_secs_f32(1.0), &mut events);
    assert!(events.is_empty(), "invalid bpm should not schedule events");
}

#[test]
fn engine_schedule_with_detune() {
    // Deterministic: 1 voice, prob=1.0, scale=[0], root=C4
    let configs = vec![VoiceConfig {
        waveform: Waveform::Sine,
        base_position: glam::Vec3::new(0.0, 0.0, 0.0),
        trigger_probability: 1.0,
        octave_offset: 0,
        base_duration: 0.25,
    }];
    let params = EngineParams {
        scale: &[0.0],
        root: MidiNote(60.0),
        ..EngineParams::default()
    };
    let mut engine = MusicEngine::new(configs, params, 12345);

    engine.set_detune_cents(Cents(50.0));
    let mut events = Vec::new();
    let seconds_per_beat = 60.0 / engine.params.bpm.0 as f64;
    engine.tick(Duration::from_secs_f64(seconds_per_beat / 2.0), &mut events);

    assert!(
        !events.is_empty(),
        "expected at least one event with probability=1.0"
    );

    let expected = midi_to_hz_with_detune(60.0, engine.params.detune.0);
    for ev in &events {
        assert!(
            (ev.freq.0 - expected).abs() < 1e-6,
            "scheduled freq does not include detune: got {:.6}, expected {:.6}",
            ev.freq.0,
            expected
        );
    }
}

#[test]
fn detune_round_trip_accuracy() {
    // Test that detune can be applied and removed accurately
    let midi_60 = 60.0; // C4

    // Apply various detune values and verify accuracy
    for detune in [-100.0, -50.0, -25.0, 0.0, 25.0, 50.0, 100.0] {
        let detuned_freq = midi_to_hz_with_detune(midi_60, detune);

        // The implementation adds detune to MIDI first, then converts to frequency
        // So -100¢ detune means MIDI 59.0, +100¢ detune means MIDI 61.0
        let detune_semitones = detune / 100.0;
        let adjusted_midi = midi_60 + detune_semitones;
        let expected_freq = midi_to_hz(adjusted_midi);

        assert!(
            (detuned_freq - expected_freq).abs() < 1e-6,
            "Detune of {detune}¢ should produce frequency for MIDI {adjusted_midi:.1}"
        );
    }
}

#[test]
fn engine_step_sequence_is_a_stable_snapshot() {
    // The engine is deterministic for a given seed; freeze the generated sequence so
    // any accidental change to note generation is caught by the host tests.
    let mut engine = make_engine(); // seed 42, default voices + C major pentatonic
    let mut events = Vec::new();
    for _ in 0..64 {
        engine.step(&mut events);
    }
    let seq: Vec<(usize, u32)> = events
        .iter()
        .map(|e| (e.voice.0, e.freq.0.round() as u32))
        .collect();
    let head: Vec<(usize, u32)> = seq.iter().take(8).copied().collect();

    // Golden sequence for seed 42 over 64 grid steps.
    assert_eq!(
        head,
        vec![
            (0, 196),
            (1, 523),
            (2, 523),
            (2, 784),
            (1, 523),
            (2, 1047),
            (1, 523),
            (2, 784),
        ],
        "engine note generation changed; len={} seq={seq:?}",
        seq.len()
    );
    assert_eq!(seq.len(), 80, "engine event count changed: {}", seq.len());
}

#[test]
fn harmony_color_maps_root_to_hue() {
    // Root pitch class drives hue_shift (0..1); C is 0.
    let (hue_c, _) = harmony_color(MidiNote(60.0), C_MAJOR_PENTATONIC);
    assert!((hue_c - 0.0).abs() < 1e-6, "C should map to hue 0");
    let (hue_cs, _) = harmony_color(MidiNote(61.0), C_MAJOR_PENTATONIC);
    assert!(
        (hue_cs - 1.0 / 12.0).abs() < 1e-6,
        "C# should map to hue 1/12"
    );
    // Octave-equivalent roots share a hue.
    let (hue_c5, _) = harmony_color(MidiNote(72.0), C_MAJOR_PENTATONIC);
    assert!(
        (hue_c5 - hue_c).abs() < 1e-6,
        "octave-equivalent roots share a hue"
    );
}

#[test]
fn harmony_color_warmth_tracks_mode_brightness() {
    let (_, ionian) = harmony_color(MidiNote(60.0), IONIAN);
    let (_, aeolian) = harmony_color(MidiNote(60.0), AEOLIAN);
    let (_, lydian) = harmony_color(MidiNote(60.0), LYDIAN);
    let (_, locrian) = harmony_color(MidiNote(60.0), LOCRIAN);
    assert!(
        ionian > aeolian,
        "major modes should read warmer than minor"
    );
    assert!(lydian >= ionian, "Lydian is the brightest mode");
    assert!(locrian < aeolian, "Locrian is the darkest mode");
    // Every mode and the pentatonic default stay within range.
    for scale in [
        IONIAN,
        DORIAN,
        PHRYGIAN,
        LYDIAN,
        MIXOLYDIAN,
        AEOLIAN,
        LOCRIAN,
        C_MAJOR_PENTATONIC,
    ] {
        let (hue, warmth) = harmony_color(MidiNote(67.0), scale);
        assert!((0.0..1.0).contains(&hue), "hue stays in [0,1)");
        assert!((0.0..=1.0).contains(&warmth), "warmth stays in [0,1]");
    }
}

#[test]
fn pitch_norm_rises_with_pitch_and_clamps() {
    let low = pitch_norm(midi_to_hz(48.0));
    let mid = pitch_norm(midi_to_hz(66.0));
    let high = pitch_norm(midi_to_hz(84.0));
    assert!(low < mid && mid < high, "pitch_norm rises with pitch");
    assert!((low - 0.0).abs() < 1e-6, "bottom of range maps to 0");
    assert!((high - 1.0).abs() < 1e-6, "top of range maps to 1");
    // Out-of-range and non-positive inputs stay clamped to [0,1].
    assert_eq!(pitch_norm(midi_to_hz(120.0)), 1.0);
    assert_eq!(pitch_norm(midi_to_hz(12.0)), 0.0);
    assert_eq!(pitch_norm(0.0), 0.0);
}

#[test]
fn engine_randomize_root_and_mode_is_seeded_and_in_range() {
    let mut e1 = make_engine();
    let mut e2 = make_engine();
    e1.randomize_root_and_mode();
    e2.randomize_root_and_mode();
    // Deterministic for a given seed (host-testable, unlike Math::random).
    assert_eq!(e1.params.root, e2.params.root);
    assert_eq!(e1.params.scale, e2.params.scale);
    // Always a valid musical root and mode.
    assert!(ROOTS_MUSICAL_ORDER.contains(&e1.params.root));
    assert!(MODES_ORDER.contains(&e1.params.scale));
}
