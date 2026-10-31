use prpr::core::{find_cfg_file, find_effect_file, is_cfg_file, is_effect_file, BeatSwitch, PlaylistCfg, RealtimeFx};

#[test]
fn detects_effect_extension() {
    assert!(is_effect_file("visual-fx.pbfx"));
    assert!(is_effect_file("EFFECT.PBFX"));
    assert!(!is_effect_file("background.png"));
    assert!(!is_effect_file("pbfx"));
    assert!(!is_effect_file("chart.json"));
}

#[test]
fn finds_effect_file_in_chart_root() {
    let files = vec![
        "info.yml".to_owned(),
        "chart.json".to_owned(),
        "cover.png".to_owned(),
        "my-fx.pbfx".to_owned(),
    ];
    assert_eq!(find_effect_file(&files).as_deref(), Some("my-fx.pbfx"));
    assert!(find_effect_file(&["info.yml".to_owned()]).is_none());
}

#[test]
fn partial_file_falls_back_to_defaults() {
    let fx: RealtimeFx = serde_json::from_str("{}").unwrap();
    assert_eq!(fx.version, 1);
    assert_eq!(fx.burst.bpm, 64.);
    assert_eq!(fx.limit.shapes, 120);
    assert_eq!(fx.ring.count, [1, 2]);
}

#[test]
fn parses_camel_case_overrides() {
    let fx: RealtimeFx = serde_json::from_str(
        r##"{
            "version": 2,
            "background": { "inner": "#112233", "hueDrift": 0 },
            "burst": { "bpm": 128, "count": [4, 6], "types": ["star"] },
            "ring": { "enabled": false }
        }"##,
    )
    .unwrap();
    assert_eq!(fx.version, 2);
    assert_eq!(fx.background.inner, "#112233");
    assert_eq!(fx.burst.bpm, 128.);
    assert_eq!(fx.burst.count, [4., 6.]);
    assert!(!fx.ring.enabled);
    assert_eq!(fx.particle.per_shape, 0.9);
    assert_eq!(fx.burst.lifetime, [2.2, 4.0]);
}

#[test]
fn parses_full_effect_file() {
    let data = r##"{
      "version": 1,
      "background": { "inner": "#2a1830", "mid": "#0b0810", "outer": "#040309", "hueDrift": 4.0, "radius": 0.75, "brightness": 2.6 },
      "palette": [8, 38, 60, 150, 190, 215, 265, 300, 330],
      "stars": { "enabled": true, "density": 1.0, "radius": [0.5, 1.9], "twinkle": [0.2, 1.0], "alpha": 0.7 },
      "ambient": {
        "enabled": true, "count": 12,
        "types": ["diamond", "hex", "tri", "circle", "star", "ring"],
        "radius": [0.028, 0.09], "spin": 0.16, "breathe": [0.22, 0.6],
        "drift": 0.004, "alpha": 0.15, "width": 1.3
      },
      "burst": {
        "enabled": true, "bpm": 64, "bpmVariance": 0.06, "count": [8, 13],
        "spread": 0.28, "startRadius": [0.004, 0.012], "endRadius": [0.018, 0.065],
        "lifetime": [2.2, 4.0], "spin": 1.1, "alpha": [0.18, 0.55], "hueJitter": 30,
        "types": ["diamond", "hex", "tri", "circle", "star", "ring"], "margin": 0.12
      },
      "ring": {
        "enabled": true, "count": [1, 2], "startRadius": [0.014, 0.036],
        "endRadius": [0.57, 0.89], "lifetime": [3.4, 5.4], "width": [1.0, 2.0],
        "alpha": [0.14, 0.24], "delay": 0.2, "fadeIn": 0.125, "hueJitter": 20
      },
      "particle": {
        "enabled": true, "perShape": 0.9, "speed": [0.028, 0.14],
        "lifetime": [1.2, 2.8], "radius": [1.0, 3.4], "drag": [0.7, 1.6],
        "hueJitter": 40, "alpha": 0.8
      },
      "floater": {
        "enabled": true, "count": 2, "types": ["star4"],
        "radius": [0.05, 0.07], "speed": [0.03, 0.05], "wander": 1.1,
        "spin": 0.5, "width": 2.2, "alpha": 0.6,
        "trail": { "enabled": true, "spacing": 0.008, "radius": [0.004, 0.009], "life": [1.4, 2.8], "alpha": 0.55, "hueJitter": 16 }
      },
      "limit": { "shapes": 120, "particles": 260, "rings": 12, "trails": 260 }
    }"##;
    let fx: RealtimeFx = serde_json::from_str(data).unwrap();
    assert_eq!(fx.version, 1);
    assert_eq!(fx.palette.len(), 9);
    assert_eq!(fx.burst.bpm, 64.);
    assert_eq!(fx.burst.count, [8., 13.]);
    assert_eq!(fx.limit.rings, 12);
    assert_eq!(fx.limit.trails, 260);
    assert_eq!(fx.floater.count, 2.);
    assert_eq!(fx.floater.types, vec!["star4".to_owned()]);
    assert!(fx.floater.trail.enabled);
    assert_eq!(fx.floater.trail.spacing, 0.008);
    assert_eq!(fx.floater.trail.scatter, 0.012);
    assert_eq!(fx.floater.wander_jerk, 0.8);
    assert_eq!(fx.background.hue_drift, 4.);
    assert_eq!(fx.background.brightness, 2.6);
    assert_eq!(fx.ring.end_radius, [0.57, 0.89]);
}

#[test]
fn detects_cfg_extension() {
    assert!(is_cfg_file("fx.cfg"));
    assert!(is_cfg_file("FX.CFG"));
    assert!(!is_cfg_file("visual-fx.pbfx"));
    assert!(!is_cfg_file("cfg"));

    let files = vec!["info.yml".to_owned(), "a.pbfx".to_owned(), "fx.cfg".to_owned()];
    assert_eq!(find_cfg_file(&files).as_deref(), Some("fx.cfg"));
    assert!(find_cfg_file(&["a.pbfx".to_owned()]).is_none());
}

#[test]
fn parses_cfg_mode1() {
    let cfg = PlaylistCfg::parse("# 播放列表\nfiles = a.pbfx, b\norder = random\nmode = 1\nbeats = 8\n").unwrap();
    assert_eq!(cfg.files, vec!["a.pbfx".to_owned(), "b.pbfx".to_owned()]);
    assert!(cfg.random);
    assert_eq!(cfg.mode, 1);
    assert_eq!(cfg.beats, vec![8.]);
    assert_eq!(cfg.color, None);
}

#[test]
fn parses_cfg_color() {
    let cfg = PlaylistCfg::parse("files = a.pbfx\ncolor = #221a33").unwrap();
    assert_eq!(cfg.color, Some([0x22, 0x1a, 0x33]));
    assert_eq!(cfg.sync_offset, 0.);

    let cfg = PlaylistCfg::parse("files = a.pbfx\nsync = 0.2").unwrap();
    assert_eq!(cfg.sync_offset, 0.2);

    let cfg = PlaylistCfg::parse("files = a.pbfx\nsyncOffset = 120ms").unwrap();
    assert!((cfg.sync_offset - 0.12).abs() < 1e-6);

    let cfg = PlaylistCfg::parse("files = a.pbfx\ncolor = 2af").unwrap();
    assert_eq!(cfg.color, Some([0x22, 0xaa, 0xff]));

    let cfg = PlaylistCfg::parse("files = a.pbfx\ncolor = not-a-color").unwrap();
    assert_eq!(cfg.color, None);
}

#[test]
fn parses_cfg_mode2_beats() {
    // 支持 [1,0,1] 与 1,0,1 两种写法
    let cfg = PlaylistCfg::parse("files=a.pbfx\nmode=2\nbeats = [16, 32, 48]").unwrap();
    assert_eq!(cfg.mode, 2);
    assert_eq!(cfg.beats, vec![16., 32., 48.]);

    let cfg = PlaylistCfg::parse("files=a.pbfx\nmode=2\nbeats=1,0,1").unwrap();
    assert_eq!(cfg.beats, vec![1., 0., 1.]);
    assert!(!cfg.random);
}

#[test]
fn rejects_invalid_cfg() {
    assert!(PlaylistCfg::parse("mode = 1\nbeats = 8").is_err());
    assert!(PlaylistCfg::parse("files = a.pbfx\nmode = 3").is_err());
}

#[test]
fn beats_field_defaults_to_8() {
    let cfg = PlaylistCfg::parse("files = a.pbfx").unwrap();
    assert_eq!(cfg.mode, 1);
    assert_eq!(cfg.beats, vec![8.]);
}

#[test]
fn switches_every_n_beats() {
    let cfg = PlaylistCfg::parse("files = a.pbfx, b.pbfx\nmode = 1\nbeats = 2").unwrap();
    let mut switcher = BeatSwitch::new(&cfg);
    assert_eq!(switcher.advance(0.0), 0); // 首帧只初始化
    assert_eq!(switcher.advance(1.9), 0);
    assert_eq!(switcher.advance(2.0), 1);
    assert_eq!(switcher.advance(3.0), 0);
    assert_eq!(switcher.advance(4.0), 1);
    assert_eq!(switcher.advance(9.0), 2);
}

#[test]
fn switches_at_given_beats() {
    let cfg = PlaylistCfg::parse("files = a.pbfx, b.pbfx\nmode = 2\nbeats = 1,0,1").unwrap();
    let mut switcher = BeatSwitch::new(&cfg);
    assert_eq!(switcher.advance(0.0), 0); // 首帧只初始化
    assert_eq!(switcher.advance(0.0), 1); // 拍点 0
    assert_eq!(switcher.advance(0.5), 0);
    assert_eq!(switcher.advance(1.0), 1); // 拍点 1
    assert_eq!(switcher.advance(5.0), 0);
}

#[test]
fn switches_align_to_beat_zero() {
    // 计时起点必须是第 0 拍：首个非负拍点只做初始化，之后严格按间隔推进
    let cfg = PlaylistCfg::parse("files = a.pbfx, b.pbfx\nmode = 1\nbeats = 4").unwrap();
    let mut switcher = BeatSwitch::new(&cfg);
    assert_eq!(switcher.advance(0.0), 0);
    assert_eq!(switcher.advance(3.9), 0);
    assert_eq!(switcher.advance(4.0), 1);
    assert_eq!(switcher.advance(8.0), 1);
}

#[test]
fn switches_snap_to_absolute_beat_grid() {
    // 即使初始化时已经过了一点拍（例如音频启动延迟），切换点仍落在 64 的整数倍
    let cfg = PlaylistCfg::parse("files = a.pbfx, b.pbfx\nmode = 1\nbeats = 64").unwrap();
    let mut switcher = BeatSwitch::new(&cfg);
    assert_eq!(switcher.advance(2.9), 0);
    assert_eq!(switcher.advance(63.9), 0);
    assert_eq!(switcher.advance(64.0), 1);
    assert_eq!(switcher.advance(127.0), 0);
    assert_eq!(switcher.advance(128.0), 1);

    // 从中间开始播放（跳转）时，下一次切换仍在网格点上
    let mut switcher = BeatSwitch::new(&cfg);
    assert_eq!(switcher.advance(100.0), 0);
    assert_eq!(switcher.advance(127.9), 0);
    assert_eq!(switcher.advance(128.0), 1);
}

#[test]
fn backward_seek_resets_switcher() {
    let cfg = PlaylistCfg::parse("files = a.pbfx, b.pbfx\nmode = 2\nbeats = 4,8").unwrap();
    let mut switcher = BeatSwitch::new(&cfg);
    assert_eq!(switcher.advance(0.0), 0);
    assert_eq!(switcher.advance(9.0), 2);
    assert_eq!(switcher.advance(0.0), 0); // 回退后重置
    assert_eq!(switcher.advance(8.0), 2); // 重新经过 4、8
}
