use rusty_engine::prelude::bevy::prelude::Color;
use rusty_engine::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};
use std::path::PathBuf;

/// Rotates `current` toward `target` by at most `max_step` radians, taking the shorter way
/// round, and keeps the result in (-PI, PI].
fn turn_toward(current: f32, target: f32, max_step: f32) -> f32 {
    let diff = (target - current + PI).rem_euclid(2.0 * PI) - PI;
    let turned = current + diff.clamp(-max_step, max_step);
    (turned + PI).rem_euclid(2.0 * PI) - PI
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Menu, // start screen, with an AI flying in the background
    Playing,
    Paused,
    Dying, // the bird has crashed and tumbles out of the window before the menu comes back
}

/// The screens of the menu. The start menu and the pause menu are each a home page, and both can
/// open Options and How to Play.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Page {
    Main,
    Pause,
    Options,
    HowToPlay,
}

/// One thing on a menu page that can be chosen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Item {
    Play,
    Resume,
    Restart,
    Options,
    HowToPlay,
    MainMenu,
    Quit,
    Back,
    Setting(usize), // index into the settings tables
}

impl Page {
    fn items(self) -> &'static [Item] {
        match self {
            Page::Main => &[Item::Play, Item::Options, Item::HowToPlay, Item::Quit],
            Page::Pause => &[
                Item::Resume,
                Item::Restart,
                Item::Options,
                Item::HowToPlay,
                Item::MainMenu,
            ],
            Page::Options => &[
                Item::Setting(FREQUENCY),
                Item::Setting(GAP),
                Item::Setting(DIFFICULTY),
                Item::Setting(MUSIC_SETTING),
                Item::Setting(SOUND_SETTING),
                Item::Back,
            ],
            Page::HowToPlay => &[Item::Back],
        }
    }

    /// Height of the `index`th item on this page; used both to draw it and to find it under the mouse.
    fn item_y(self, index: usize) -> f32 {
        let i = index as f32;
        match self {
            Page::Main => 25.0 - 60.0 * i,
            Page::Pause => 60.0 - 55.0 * i,
            Page::Options => 90.0 - 48.0 * i,
            Page::HowToPlay => -145.0,
        }
    }

    /// The item under the mouse, if any.
    fn item_at(self, mouse: Vec2) -> Option<usize> {
        if mouse.x.abs() > PANEL_HALF_W {
            return None;
        }
        (0..self.items().len()).find(|&i| (mouse.y - self.item_y(i)).abs() < ITEM_HIT_HALF_H)
    }
}

/// Turns real frame times into a whole number of fixed game steps, so the game runs at the same
/// speed on any monitor. A frame time within a few percent of a simple multiple or fraction of a
/// step is snapped to it, so timing jitter on a 30, 60, 120 or 240 Hz screen never causes a skipped
/// or doubled step.
#[derive(Default)]
struct StepClock {
    banked: f64, // time not yet turned into steps, measured in steps
}

impl StepClock {
    fn steps(&mut self, frame_secs: f32) -> u32 {
        let mut frame = frame_secs as f64 * STEPS_PER_SECOND;
        if let Some(snap) = [0.25, 1.0 / 3.0, 0.5, 1.0, 2.0, 3.0, 4.0]
            .into_iter()
            .find(|&s| (frame - s).abs() < s * STEP_SNAP)
        {
            frame = snap;
        }
        self.banked = (self.banked + frame).min(MAX_STEPS_PER_FRAME as f64);
        let steps = (self.banked + 1e-6).floor();
        self.banked -= steps;
        steps as u32
    }
}

#[derive(Resource)]
struct GameState {
    score: f32,                   // points earned this run; shown rounded down
    pipes: u32,                   // pipes the bird has flown through this run
    passed: [bool; NUM_PILLARS],  // which pillars the bird is already past, so each counts once
    last_run: Option<(u32, u32)>, // (points, pipes) of the run that just ended
    high_score: u32,
    mode: Mode,
    page: Page,      // which menu page is showing, while in the menu or paused
    selected: usize, // which item on the page the menu cursor is on
    levels: [usize; SETTING_COUNT], // chosen level of each setting, indexes into the tables below
    clock: StepClock,
    lift: f32,
    glide_speed: f32,
    stalled: bool,
    heading: f32, // flight direction in radians: 0 is forward, positive is up
    prev_flap: bool,
    death_frame: u32,      // steps since the bird crashed, while dying
    death_fall_speed: f32, // the crashed bird's vertical speed; positive is up
    view_scale: f32,       // how much the world was scaled to fit the window last frame
}

impl GameState {
    fn new() -> Self {
        GameState {
            score: 0.0,
            pipes: 0,
            passed: [false; NUM_PILLARS],
            last_run: None,
            high_score: 0,
            mode: Mode::Menu,
            page: Page::Main,
            selected: 0,
            levels: [DEFAULT_LEVEL; SETTING_COUNT],
            clock: StepClock::default(),
            lift: 0.0,
            glide_speed: GLIDE_BASE_SPEED,
            stalled: false,
            heading: 0.0,
            prev_flap: false,
            death_frame: 0,
            death_fall_speed: 0.0,
            view_scale: 1.0,
        }
    }

    /// Puts the bird's flight state back to a fresh start.
    fn reset_bird(&mut self) {
        self.lift = 0.0;
        self.glide_speed = GLIDE_BASE_SPEED;
        self.stalled = false;
        self.heading = 0.0;
        self.prev_flap = false;
    }

    fn pillar_spacing(&self) -> f32 {
        PILLAR_SPACING[self.levels[FREQUENCY]]
    }

    fn gap_size(&self) -> f32 {
        BASE_GAP * GAP_SCALE[self.levels[GAP]]
    }

    fn speed_scale(&self) -> f32 {
        SPEED_SCALE[self.levels[DIFFICULTY]]
    }

    fn difficulty_multiplier(&self) -> f32 {
        DIFFICULTY_MULTIPLIER[self.levels[DIFFICULTY]] * self.gap_multiplier()
    }

    /// Reward for a tight gap: the smaller the gap, the more every point is worth.
    fn gap_multiplier(&self) -> f32 {
        1.0 / GAP_SCALE[self.levels[GAP]]
    }

    /// Whole points earned so far this run.
    fn points(&self) -> u32 {
        self.score as u32
    }

    /// How much the time-based point rate is multiplied by right now: it grows towards the front
    /// of the window, with flight speed, with the difficulty, and with every pipe passed.
    fn score_multiplier(&self, bird_x: f32) -> f32 {
        front_factor(bird_x)
            * speed_factor(self.glide_speed)
            * self.difficulty_multiplier()
            * (1.0 + self.pipes as f32 * PIPE_STREAK_BONUS)
    }

    /// Resets the points and the pipe count for a fresh run.
    fn reset_score(&mut self) {
        self.score = 0.0;
        self.pipes = 0;
        self.passed = [false; NUM_PILLARS];
    }

    /// Remembers how the run went (for the menu) and updates the high score.
    fn end_run(&mut self) {
        let points = self.points();
        self.high_score = self.high_score.max(points);
        self.last_run = Some((points, self.pipes));
    }

    /// Starts the death animation.
    fn start_dying(&mut self) {
        self.mode = Mode::Dying;
        self.death_frame = 0;
        self.death_fall_speed = DEATH_POP_SPEED;
    }

    /// Shows a menu page with the cursor on its first item.
    fn open_page(&mut self, page: Page) {
        self.page = page;
        self.selected = 0;
    }

    /// The page that Back returns to: the pause menu mid-run, the start menu otherwise.
    fn home_page(&self) -> Page {
        if self.mode == Mode::Paused {
            Page::Pause
        } else {
            Page::Main
        }
    }

    fn music_volume(&self) -> f32 {
        MUSIC_VOLUMES[self.levels[MUSIC_SETTING]]
    }

    fn sound_volume(&self) -> f32 {
        SOUND_VOLUMES[self.levels[SOUND_SETTING]]
    }
}

/// Point multiplier for how far forward the bird is: x1 at the back of its range, up to
/// x(1 + FRONT_BONUS) at the front.
fn front_factor(bird_x: f32) -> f32 {
    let t = ((bird_x - PLAYER_MIN_X) / (PLAYER_MAX_X - PLAYER_MIN_X)).clamp(0.0, 1.0);
    1.0 + FRONT_BONUS * t
}

/// Point multiplier for flight speed: x1 at cruising speed, more while gliding fast.
fn speed_factor(glide_speed: f32) -> f32 {
    (glide_speed / GLIDE_BASE_SPEED).max(1.0)
}

/// Marks every pillar the bird has now flown completely past and returns how many are new.
/// `passed` is cleared by `scroll_world` when a pillar is recycled to the right.
fn count_passed(
    pillar_xs: &[f32; NUM_PILLARS],
    passed: &mut [bool; NUM_PILLARS],
    bird_x: f32,
) -> u32 {
    let mut newly_passed = 0;
    for i in 0..NUM_PILLARS {
        if !passed[i] && pillar_xs[i] + PILLAR_HALF_W + BIRD_HALF_W < bird_x {
            passed[i] = true;
            newly_passed += 1;
        }
    }
    newly_passed
}

// Menu settings. Each one has LEVELS steps; the middle step is the original game's value.
const LEVELS: usize = 5;
const DEFAULT_LEVEL: usize = 2;
const SETTING_COUNT: usize = 5;
const FREQUENCY: usize = 0;
const GAP: usize = 1;
const DIFFICULTY: usize = 2;
const MUSIC_SETTING: usize = 3;
const SOUND_SETTING: usize = 4;
const SETTING_NAMES: [&str; SETTING_COUNT] = [
    "Pillar frequency",
    "Gap size",
    "Difficulty",
    "Music",
    "Sound effects",
];
const VOLUME_NAMES: [&str; LEVELS] = ["Off", "Quiet", "Normal", "Loud", "Max"];
const LEVEL_NAMES: [[&str; LEVELS]; SETTING_COUNT] = [
    ["Very rare", "Rare", "Normal", "Frequent", "Very frequent"],
    ["Tiny", "Small", "Normal", "Large", "Huge"],
    ["Very easy", "Easy", "Normal", "Hard", "Very hard"],
    VOLUME_NAMES,
    VOLUME_NAMES,
];
const PILLAR_SPACING: [f32; LEVELS] = [900.0, 750.0, 650.0, 550.0, 450.0]; // x distance between pillars
const GAP_SCALE: [f32; LEVELS] = [0.6, 0.8, 1.0, 1.2, 1.4];
const SPEED_SCALE: [f32; LEVELS] = [0.6, 0.8, 1.0, 1.25, 1.5]; // difficulty: how fast the pillars go by
const MUSIC_VOLUMES: [f32; LEVELS] = [0.0, 0.2, 0.4, 0.6, 0.8];
const SOUND_VOLUMES: [f32; LEVELS] = [0.0, 0.5, 1.0, 1.15, 1.25]; // scales each sound's own volume

// The game advances in fixed steps of 1/60 s whatever the monitor's refresh rate, so every speed
// below that says "per frame" means per step.
const STEPS_PER_SECOND: f64 = 60.0;
const MAX_STEPS_PER_FRAME: u32 = 4; // after a long hitch, catch up this much at most instead of leaping ahead
const STEP_SNAP: f64 = 0.05; // how close (as a fraction) a frame time must be to a simple step multiple to snap

// Points. The score ticks up every frame at BASE_POINTS_PER_FRAME times a multiplier, and each
// pipe passed adds a lump sum on top.
const BASE_POINTS_PER_FRAME: f32 = 0.2;
const FRONT_BONUS: f32 = 2.0; // extra multiplier at the very front of the window (x3 in total)
const PIPE_STREAK_BONUS: f32 = 0.1; // each pipe passed adds this much to the multiplier
const PIPE_POINTS: f32 = 50.0; // lump sum for passing a pipe, before the difficulty multiplier
const DIFFICULTY_MULTIPLIER: [f32; LEVELS] = [0.5, 0.75, 1.0, 1.5, 2.0]; // points scale with difficulty
const BASE_GAP: f32 = 420.0;
const BASE_SCROLL_SPEED: f32 = 6.0; // floor and pillars, pixels per frame
const BASE_BACKGROUND_SPEED: f32 = 3.0;

const LIFT_ON_RELEASE: f32 = 18.0;
const LIFT_DECAY: f32 = 0.8;
const GRAVITY: f32 = 10.0;
const FLAP_LIFT: f32 = 20.0;
const STAND_Y: f32 = -295.0;
const GLIDE_BASE_SPEED: f32 = 6.0;
const GLIDE_MIN_SPEED: f32 = 2.0;
const GLIDE_MAX_SPEED: f32 = 18.0;
const GLIDE_DIVE_ACCEL: f32 = 0.6;
const GLIDE_CLIMB_DRAG: f32 = 0.8;
const GLIDE_COAST_DRAG: f32 = 0.5;
const GLIDE_STALL_SPEED: f32 = 3.0;
const GLIDE_RECOVER_SPEED: f32 = 8.0; // a stalled bird must fall until it reaches this to steer again
const STALL_DIVE_ACCEL: f32 = 0.2; // speed regained per frame while falling out of a stall
const STALL_NOSE_DROP: f32 = 0.06; // radians per frame the nose drops toward straight down
const TURN_RATE_PER_SPEED: f32 = 0.008; // radians per frame of turning, per unit of speed
const GLIDE_ARRIVE_DIST: f32 = 15.0; // stop re-aiming when the mouse is this close
const MAX_TILT: f32 = 0.8; // radians, about 45 degrees
const STALL_TILT: f32 = -1.0; // nose down while stalled
const TILT_SMOOTHING: f32 = 0.15;
const WIND_SPEED: f32 = 1.0;
const PLAYER_START_X: f32 = -450.0;
const WINDOW_WIDTH: u32 = 1440;
const WINDOW_HEIGHT: u32 = 810;
const PLAYER_MARGIN: f32 = 40.0; // keeps the bird's sprite fully inside the window
const PLAYER_MIN_X: f32 = -(WINDOW_WIDTH as f32) / 2.0 + PLAYER_MARGIN;
const PLAYER_MAX_X: f32 = WINDOW_WIDTH as f32 / 2.0 - PLAYER_MARGIN;
const PLAYER_MIN_Y: f32 = -(WINDOW_HEIGHT as f32) / 2.0 + PLAYER_MARGIN;
const PLAYER_MAX_Y: f32 = WINDOW_HEIGHT as f32 / 2.0 - PLAYER_MARGIN;

// Collision extents of the bird and the floor, in world pixels (collider polygon * sprite scale).
const BIRD_HALF_W: f32 = 30.5;
#[cfg(test)]
const BIRD_HALF_H: f32 = 22.5;
#[cfg(test)]
const FLOOR_TOP_Y: f32 = -305.0;

const BACKGROUND: f32 = 0.0;
const PILLAR: f32 = 0.5; // behind the floor, so a pillar's lower end is hidden in the ground
const FLOOR: f32 = 1.0;
const USER: f32 = 2.0;
const PANEL: f32 = 800.0;
const PILLAR_RECYCLE_X: f32 = -(WINDOW_WIDTH as f32) / 2.0 - 120.0; // just past the left edge

// The background and the floor are each two copies of one image laid side by side, which
// leapfrog each other as they scroll so the image repeats forever.
const BACKGROUND_LABELS: [&str; 2] = ["background", "background2"];
const FLOOR_LABELS: [&str; 2] = ["floor", "floor2"];
const TILE_SCALE: f32 = 1.7;
const TILE_WIDTH: f32 = 1600.0 * TILE_SCALE;
const FLOOR_Y: f32 = -385.0;

// Every pillar is the same tall sprite. The gap's size and height are set by sliding the bottom
// pillar down and the top pillar up, and the ends that slide out of view are hidden behind the
// floor and past the top of the window.
const PILLAR_IMAGE: &str = "sprite/flutter/long_pillar.png";
const PILLAR_SCALE: f32 = 0.3;
const PILLAR_LENGTH: f32 = 1120.0 * PILLAR_SCALE;
const PILLAR_EDGE: f32 = 168.0; // sprite centre to its gap-facing collider edge (559.98 * scale)
const PILLAR_HALF_W: f32 = 39.0;
const NUM_PILLARS: usize = 5;
const FIRST_PILLAR_X: f32 = 1000.0;
const FLOOR_HIDE_Y: f32 = -310.0; // a bottom pillar's lower end must reach at least this far down
const CEILING_COVER_Y: f32 = WINDOW_HEIGHT as f32 / 2.0 + 20.0; // a top pillar must reach this high
const PILLAR_CENTERS: [f32; NUM_PILLARS] = [21.0, 122.0, 230.0, 70.0, 175.0]; // preferred gap heights

// The AI that plays behind the menu.
const AI_FLAP_BELOW: f32 = 20.0; // start flapping when this far below the gap's centre
const AI_RELEASE_ABOVE: f32 = 10.0; // stop flapping once this far above it
const AI_CLEARANCE: f32 = 10.0; // keep aiming at a pillar's gap until this far past it
const AI_MIN_Y: f32 = -230.0;
const AI_MAX_Y: f32 = 330.0;

// Sounds, as paths inside assets/audio/. Volumes run from 0.0 to 1.0; the sound effects' volumes
// are at the Normal setting and the Sound effects setting scales them.
const MUSIC: &str = "music/Classy 8-Bit.ogg"; // loops the whole time the game is open
const PIPE_SOUND: &str = "sfx/confirmation2.ogg"; // a bright ding for every pipe flown through
const PIPE_SOUND_VOLUME: f32 = 0.6;
const DEATH_SOUND: &str = "sfx/jingle3.ogg"; // four notes sliding down: "wah, wah, wah, wahhh"
const DEATH_SOUND_VOLUME: f32 = 0.8;

// The death animation: the bird freezes and shakes on impact, then pops up and tumbles out of the
// bottom of the window. The menu comes back as the death sound finishes.
const DEATH_FREEZE_FRAMES: u32 = 14; // even, so the shake ends where the bird crashed
const DEATH_SHAKE: f32 = 4.0;
const DEATH_POP_SPEED: f32 = 12.0;
const DEATH_GRAVITY: f32 = 0.7;
const DEATH_SPIN: f32 = 0.25; // radians per frame
const DEATH_OFFSCREEN_Y: f32 = -(WINDOW_HEIGHT as f32) / 2.0 - 60.0; // the bird is out of view below this
const DEATH_FRAMES: u32 = 105; // 1.75 s, as long as the death sound

const MENU_FONT: &str = "font/BitcountPropSingle-VariableFont_CRSV,ELSH,ELXP,slnt,wght.ttf";
const OFFSCREEN: Vec2 = Vec2::new(-5000.0, 0.0);
const PANEL_HALF_W: f32 = 450.0; // the menu panel image is 900 x 560
const ITEM_HIT_HALF_H: f32 = 24.0; // how close to an item's centre line the mouse must be to pick it

// Text the menu pages are drawn with. Each page decides what goes on which line, and where.
const MENU_TITLE: &str = "menu_title";
const MENU_SUBTITLE: &str = "menu_subtitle";
const MENU_INFO: &str = "menu_info";
const MENU_HINT: &str = "menu_hint";
const MENU_LINES: [&str; 8] = [
    "menu_line_0",
    "menu_line_1",
    "menu_line_2",
    "menu_line_3",
    "menu_line_4",
    "menu_line_5",
    "menu_line_6",
    "menu_line_7",
];
const ITEM_FONT_SIZE: f32 = 34.0;
const SELECTED_FONT_SIZE: f32 = 40.0;
const SETTING_FONT_SIZE: f32 = 26.0;
const SELECTED_SETTING_FONT_SIZE: f32 = 30.0;

// (label, y, font size) of the score lines along the top of the window
const HUD_TEXTS: [(&str, f32, f32); 2] = [("hud_score", 365.0, 40.0), ("hud_pipes", 322.0, 26.0)];

// The speed bar on the ground during a run: a track from the slowest glide to the fastest, a red
// zone where climbing stalls the bird, and a needle at the current speed. The engine can't tint or
// stretch a sprite, so every piece is a solid-coloured image of its own, made at startup.
const GENERATED_IMAGES: &str = "sprite/generated";
const SPEED_BAR_Y: f32 = -362.0;
const SPEED_BAR_W: f32 = 420.0;
const SPEED_BAR_H: f32 = 14.0;
const SPEED_BACK_CENTER: Vec2 = Vec2::new(0.0, -361.0);
const SPEED_BACK_SIZE: Vec2 = Vec2::new(600.0, 84.0);
const SPEED_NEEDLE_SIZE: Vec2 = Vec2::new(4.0, 26.0);
const SPEED_RECOVER_SIZE: Vec2 = Vec2::new(3.0, 22.0);
const SPEED_LABEL_Y: f32 = -388.0;
const SPEED_HUD: f32 = 850.0; // layer: in front of the world and the bird, behind the text
const SPEED_BACK_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const SPEED_TRACK_COLOR: Color = Color::srgb(0.22, 0.22, 0.22);
const SPEED_OK_COLOR: Color = Color::srgb(0.35, 0.8, 1.0); // cruising speed or faster
const SPEED_SLOW_COLOR: Color = Color::srgb(1.0, 0.7, 0.2); // below cruising speed: careful climbing
const SPEED_STALL_COLOR: Color = Color::srgb(0.95, 0.2, 0.2);
const SPEED_STALL_ZONE_COLOR: Color = Color::srgb(0.7, 0.08, 0.08);
const SPEED_RECOVER_COLOR: Color = Color::srgb(1.0, 0.9, 0.3);
const SPEED_BACK: &str = "speed_back";
const SPEED_TRACK: &str = "speed_track";
const SPEED_FILL: &str = "speed_fill";
const SPEED_STALL_ZONE: &str = "speed_stall_zone";
const SPEED_RECOVER: &str = "speed_recover";
const SPEED_NEEDLE: &str = "speed_needle";
const SPEED_PIECES: [&str; 6] = [
    SPEED_BACK,
    SPEED_TRACK,
    SPEED_FILL,
    SPEED_STALL_ZONE,
    SPEED_RECOVER,
    SPEED_NEEDLE,
]; // back to front
// The fill changes width, so it is built from segments 1, 2, 4, ... 256 pixels wide (enough for
// SPEED_BAR_W), one set per colour, and shows the segments that add up to its width.
const SPEED_FILL_COLORS: [Color; 3] = [SPEED_OK_COLOR, SPEED_SLOW_COLOR, SPEED_STALL_COLOR];
const SPEED_FILL_SEGMENTS: u32 = 9;
const SPEED_TITLE: &str = "speed_title";
const SPEED_MIN_LABEL: &str = "speed_min";
const SPEED_MAX_LABEL: &str = "speed_max";
const SPEED_STALL_LABEL: &str = "speed_stall";
const SPEED_RECOVER_LABEL: &str = "speed_recover_label";

/// What the bird is being told to do this frame, from the keyboard and mouse or from the AI.
struct Controls {
    flap: bool,
    glide: bool,
    glide_target: Option<Vec2>,
}

/// The gap between one top and bottom pillar pair.
#[derive(Clone, Copy)]
struct Pair {
    x: f32,
    e_b: f32, // top edge of the bottom pillar
    e_t: f32, // bottom edge of the top pillar
}

/// Height of the middle of a gap. Pillars have a fixed length, so how high or low a gap of this
/// size can sit is limited: the bottom pillar must still reach the floor and the top pillar the
/// top of the window.
fn gap_center(index: usize, gap: f32) -> f32 {
    let min = CEILING_COVER_Y - PILLAR_LENGTH - gap / 2.0;
    let max = FLOOR_HIDE_Y + PILLAR_LENGTH + gap / 2.0;
    PILLAR_CENTERS[index % NUM_PILLARS].clamp(min, max)
}

fn pair_for(index: usize, x: f32, gap: f32) -> Pair {
    let center = gap_center(index, gap);
    Pair {
        x,
        e_b: center - gap / 2.0,
        e_t: center + gap / 2.0,
    }
}

/// Moves every pillar left. A pillar that scrolls off the left edge is sent back to the right,
/// `spacing` beyond the rightmost one; the returned flags mark which pillars were recycled.
fn scroll_pillars(xs: &mut [f32; NUM_PILLARS], speed: f32, spacing: f32) -> [bool; NUM_PILLARS] {
    let mut recycled = [false; NUM_PILLARS];
    for i in 0..NUM_PILLARS {
        let next_x = xs[i] - speed;
        if next_x < PILLAR_RECYCLE_X {
            let rightmost_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            xs[i] = rightmost_x + spacing;
            recycled[i] = true;
        } else {
            xs[i] = next_x;
        }
    }
    recycled
}

/// Advances the bird's flight by one frame: gravity, flapping, gliding, tilt, wind and the
/// window edges. The player and the menu's AI both fly through this.
fn step_bird(pos: &mut Vec2, rotation: &mut f32, gs: &mut GameState, c: &Controls) {
    // Gravity is suspended while gliding; the glide's own speed model takes over (unless stalled).
    if pos.y > STAND_Y && (!c.glide || gs.stalled) {
        pos.y -= GRAVITY;
    }

    if c.flap {
        pos.y += FLAP_LIFT;
    }

    // Momentum: releasing the flap starts a small upward carry-over that fades out,
    // so the bird eases into falling instead of stopping dead.
    if gs.prev_flap && !c.flap {
        gs.lift = LIFT_ON_RELEASE;
    }
    gs.prev_flap = c.flap;

    if gs.lift > 0.0 {
        pos.y += gs.lift;
        gs.lift *= LIFT_DECAY;
        if gs.lift < 1.0 {
            gs.lift = 0.0;
        }
    }

    // A stalled bird has lost too much speed climbing: it can't glide, so it falls (gravity
    // above) and the fall rebuilds speed until it can steer again.
    if gs.stalled {
        gs.glide_speed += STALL_DIVE_ACCEL;
        gs.heading = turn_toward(gs.heading, -FRAC_PI_2, STALL_NOSE_DROP);
        if gs.glide_speed >= GLIDE_RECOVER_SPEED {
            gs.stalled = false;
        }
    }

    // Gliding: head toward the mouse, trading height for speed. Heading downhill builds
    // speed, climbing bleeds it off, and letting go of the glide key drains it back to base.
    let mut target_tilt = if gs.stalled { STALL_TILT } else { 0.0 };
    if !gs.stalled {
        if c.glide {
            // The bird turns toward the mouse at a rate set by its speed, so a slow bird barely
            // turns at all. Speed gain and loss follow where the bird is actually pointing.
            if let Some(mouse) = c.glide_target {
                let to_mouse = mouse - *pos;
                if to_mouse.length() > GLIDE_ARRIVE_DIST {
                    let desired = to_mouse.y.atan2(to_mouse.x);
                    let turn_rate = TURN_RATE_PER_SPEED * gs.glide_speed;
                    gs.heading = turn_toward(gs.heading, desired, turn_rate);
                }
            }
            let (sin, cos) = gs.heading.sin_cos();
            if sin < 0.0 {
                gs.glide_speed += GLIDE_DIVE_ACCEL * -sin;
            } else {
                gs.glide_speed -= GLIDE_CLIMB_DRAG * sin;
            }
            gs.glide_speed = gs.glide_speed.clamp(GLIDE_MIN_SPEED, GLIDE_MAX_SPEED);
            if sin > 0.0 && gs.glide_speed <= GLIDE_STALL_SPEED {
                gs.stalled = true;
                target_tilt = STALL_TILT;
            } else {
                // Use |cos| so the nose never flips backwards: flying back is only tilted by
                // how far up or down the bird is heading.
                target_tilt = sin.atan2(cos.abs()).clamp(-MAX_TILT, MAX_TILT);
                *pos += Vec2::new(cos, sin) * gs.glide_speed;
            }
        } else {
            gs.glide_speed = (gs.glide_speed - GLIDE_COAST_DRAG).max(GLIDE_BASE_SPEED);
            gs.heading = turn_toward(gs.heading, 0.0, TURN_RATE_PER_SPEED * GLIDE_BASE_SPEED);
        }
    }
    *rotation += (target_tilt - *rotation) * TILT_SMOOTHING;

    // A small headwind pushes the bird backwards, so it has to glide forward to hold its place.
    pos.x -= WIND_SPEED;

    // Keep the bird inside the window on every side.
    pos.x = pos.x.clamp(PLAYER_MIN_X, PLAYER_MAX_X);
    pos.y = pos.y.clamp(PLAYER_MIN_Y, PLAYER_MAX_Y);
}

/// Advances the death animation by one frame. Returns true once it has finished.
fn step_death(pos: &mut Vec2, rotation: &mut f32, gs: &mut GameState) -> bool {
    if gs.death_frame < DEATH_FREEZE_FRAMES {
        pos.x += if gs.death_frame % 2 == 0 {
            DEATH_SHAKE
        } else {
            -DEATH_SHAKE
        };
    } else if pos.y > DEATH_OFFSCREEN_Y {
        pos.y += gs.death_fall_speed;
        gs.death_fall_speed -= DEATH_GRAVITY;
        *rotation += DEATH_SPIN;
    }
    gs.death_frame += 1;
    gs.death_frame >= DEATH_FRAMES
}

/// Height the AI should fly at: the middle of the next gap it still has to get through.
fn ai_target(bird_x: f32, pairs: &[Pair]) -> f32 {
    let mut next: Option<&Pair> = None;
    for pair in pairs {
        let still_ahead = pair.x + PILLAR_HALF_W + BIRD_HALF_W + AI_CLEARANCE > bird_x;
        if still_ahead && next.map_or(true, |n| pair.x < n.x) {
            next = Some(pair);
        }
    }
    next.map_or(0.0, |p| (p.e_b + p.e_t) / 2.0)
        .clamp(AI_MIN_Y, AI_MAX_Y)
}

/// The AI only flaps: it holds the flap while below the gap's centre and lets go once above it,
/// with a dead zone between the two so it bobs rather than buzzes.
fn ai_controls(bird: Vec2, was_flapping: bool, pairs: &[Pair]) -> Controls {
    let target = ai_target(bird.x, pairs);
    let flap = if was_flapping {
        bird.y < target + AI_RELEASE_ABOVE
    } else {
        bird.y < target - AI_FLAP_BELOW
    };
    Controls {
        flap,
        glide: false,
        glide_target: None,
    }
}

// Sprite labels of each pillar pair, as [bottom, top]. Kept as constants so looking a pillar up
// every frame doesn't build a new string each time.
const PILLAR_LABELS: [[&str; 2]; NUM_PILLARS] = [
    ["pillar_0_bottom", "pillar_0_top"],
    ["pillar_1_bottom", "pillar_1_top"],
    ["pillar_2_bottom", "pillar_2_top"],
    ["pillar_3_bottom", "pillar_3_top"],
    ["pillar_4_bottom", "pillar_4_top"],
];

fn pillar_label(index: usize, top: bool) -> &'static str {
    PILLAR_LABELS[index][top as usize]
}

/// Puts a pillar pair at `x` with a gap of the given size.
fn place_pillar_pair(engine: &mut Engine, index: usize, x: f32, gap: f32) {
    let pair = pair_for(index, x, gap);
    let bottom = engine.sprites.get_mut(pillar_label(index, false)).unwrap();
    bottom.translation = Vec2::new(x, pair.e_b - PILLAR_EDGE);
    let top = engine.sprites.get_mut(pillar_label(index, true)).unwrap();
    top.translation = Vec2::new(x, pair.e_t + PILLAR_EDGE);
}

/// Where each pillar pair currently is along x.
fn pillar_xs(engine: &Engine) -> [f32; NUM_PILLARS] {
    std::array::from_fn(|i| engine.sprites[pillar_label(i, false)].translation.x)
}

fn read_pairs(engine: &Engine) -> [Pair; NUM_PILLARS] {
    std::array::from_fn(|i| {
        let bottom = &engine.sprites[pillar_label(i, false)];
        let top = &engine.sprites[pillar_label(i, true)];
        Pair {
            x: bottom.translation.x,
            e_b: bottom.translation.y + PILLAR_EDGE,
            e_t: top.translation.y - PILLAR_EDGE,
        }
    })
}

/// Starts a run from scratch: the bird goes back to its starting spot and the pillars are laid
/// out again using the current settings.
fn reset_run(engine: &mut Engine, gs: &mut GameState) {
    let (spacing, gap) = (gs.pillar_spacing(), gs.gap_size());
    for i in 0..NUM_PILLARS {
        place_pillar_pair(engine, i, FIRST_PILLAR_X + i as f32 * spacing, gap);
    }
    gs.reset_bird();
    gs.reset_score();
    let player = engine.sprites.get_mut("user").unwrap();
    player.translation = Vec2::new(PLAYER_START_X, 0.0);
    player.rotation = 0.0;
}

/// Scrolls the floor, background and pillars. The difficulty setting scales all of them.
fn scroll_world(engine: &mut Engine, gs: &mut GameState) {
    let scale = gs.speed_scale();
    scroll_tiles(engine, FLOOR_LABELS, BASE_SCROLL_SPEED * scale);
    scroll_tiles(engine, BACKGROUND_LABELS, BASE_BACKGROUND_SPEED * scale);

    let mut xs = pillar_xs(engine);
    let recycled = scroll_pillars(&mut xs, BASE_SCROLL_SPEED * scale, gs.pillar_spacing());
    for i in 0..NUM_PILLARS {
        if recycled[i] {
            // A recycled pair is brand new, so it picks up the current gap size and can be
            // passed (and counted) again.
            place_pillar_pair(engine, i, xs[i], gs.gap_size());
            gs.passed[i] = false;
        } else {
            for top in [false, true] {
                engine
                    .sprites
                    .get_mut(pillar_label(i, top))
                    .unwrap()
                    .translation
                    .x = xs[i];
            }
        }
    }
}

/// Moves both copies of a repeating image left. Once one has scrolled fully off the left edge,
/// it jumps to just right of the other.
fn scroll_tiles(engine: &mut Engine, labels: [&str; 2], speed: f32) {
    for label in labels {
        engine.sprites.get_mut(label).unwrap().translation.x -= speed;
    }
    for (label, other) in [(labels[0], labels[1]), (labels[1], labels[0])] {
        let other_x = engine.sprites[other].translation.x;
        let tile = engine.sprites.get_mut(label).unwrap();
        if tile.translation.x <= -TILE_WIDTH {
            tile.translation.x = other_x + TILE_WIDTH;
        }
    }
}

/// Adds this frame's points: the time-based trickle, plus a lump sum for each pipe just cleared.
fn award_points(engine: &mut Engine, gs: &mut GameState) {
    let bird_x = engine.sprites["user"].translation.x;
    gs.score += BASE_POINTS_PER_FRAME * gs.score_multiplier(bird_x);

    let cleared = count_passed(&pillar_xs(engine), &mut gs.passed, bird_x);
    if cleared > 0 {
        play_sound(engine, gs, PIPE_SOUND, PIPE_SOUND_VOLUME);
    }
    gs.pipes += cleared;
    gs.score += cleared as f32 * PIPE_POINTS * gs.difficulty_multiplier();
}

fn player_controls(engine: &Engine) -> Controls {
    let kb = &engine.keyboard_state;
    let mouse = &engine.mouse_state;
    Controls {
        flap: kb.pressed(KeyCode::Space) || mouse.pressed(MouseButton::Left),
        glide: kb.pressed(KeyCode::KeyG) || mouse.pressed(MouseButton::Right),
        glide_target: world_mouse(engine),
    }
}

/// Where the mouse is in the game world. The engine reports it in window pixels, but the world is
/// drawn scaled to fit the window.
fn world_mouse(engine: &Engine) -> Option<Vec2> {
    engine
        .mouse_state
        .location()
        .map(|m| m / view_scale(engine))
}

/// How much to scale the world so the whole WINDOW_WIDTH x WINDOW_HEIGHT area fits the window.
fn view_scale(engine: &Engine) -> f32 {
    let window = engine.window_dimensions;
    let scale = (window.x / WINDOW_WIDTH as f32).min(window.y / WINDOW_HEIGHT as f32);
    if scale > 0.0 { scale } else { 1.0 } // no window yet
}

/// The engine's camera shows one window pixel per unit, so to always show the same area of the
/// world, however big the window or monitor is, every sprite and text is scaled to fit the window
/// at the end of each frame (`world_to_window`) and back again at the start of the next
/// (`window_to_world`). The game logic in between only ever sees world positions.
fn window_to_world(engine: &mut Engine, gs: &mut GameState) {
    rescale(engine, 1.0 / gs.view_scale);
}

fn world_to_window(engine: &mut Engine, gs: &mut GameState) {
    gs.view_scale = view_scale(engine);
    rescale(engine, gs.view_scale);
}

fn rescale(engine: &mut Engine, factor: f32) {
    if factor == 1.0 {
        return;
    }
    for sprite in engine.sprites.values_mut() {
        sprite.translation *= factor;
        sprite.scale *= factor;
    }
    for text in engine.texts.values_mut() {
        text.translation *= factor;
        text.scale *= factor;
    }
}

/// Plays a sound effect, scaled by the Sound effects setting.
fn play_sound(engine: &mut Engine, gs: &GameState, sound: &str, volume: f32) {
    let volume = volume * gs.sound_volume();
    if volume > 0.0 {
        engine.audio_manager.play_sfx(sound, volume);
    }
}

/// Starts the music at the chosen volume, or stops it if the Music setting is Off. The engine can't
/// change the volume of music that is already playing, so a new volume restarts the song.
fn apply_music_volume(engine: &mut Engine, gs: &GameState) {
    let volume = gs.music_volume();
    if volume > 0.0 {
        engine.audio_manager.play_music(MUSIC, volume);
    } else {
        engine.audio_manager.stop_music();
    }
}

/// Ends the run, keeps its score for the menu, and saves the best score.
fn finish_run(gs: &mut GameState) {
    gs.end_run();
    save_progress(gs);
}

fn start_run(engine: &mut Engine, gs: &mut GameState) {
    reset_run(engine, gs);
    gs.mode = Mode::Playing;
}

/// What the player did to the menu this frame.
struct MenuInput {
    up: bool,
    down: bool,
    activate: bool, // Enter, Space or a left click on an item
    raise: bool,    // Right / D
    lower: bool,    // Left / A, or a right click on an item
    back: bool,     // Backspace
    pause: bool,    // P
    reset: bool,    // R
}

/// Reads the keyboard and mouse for the menu. Pointing at an item puts the cursor on it, but only
/// when the mouse moves or clicks, so a resting mouse doesn't fight the arrow keys.
fn read_menu_input(engine: &Engine, gs: &mut GameState) -> MenuInput {
    let kb = &engine.keyboard_state;
    let mouse = &engine.mouse_state;
    let hovered = world_mouse(engine).and_then(|m| gs.page.item_at(m));
    let left_click = hovered.is_some() && mouse.just_pressed(MouseButton::Left);
    let right_click = hovered.is_some() && mouse.just_pressed(MouseButton::Right);
    if let Some(i) = hovered {
        if mouse.motion() != Vec2::ZERO || left_click || right_click {
            gs.selected = i;
        }
    }
    MenuInput {
        up: kb.just_pressed_any(&[KeyCode::ArrowUp, KeyCode::KeyW]),
        down: kb.just_pressed_any(&[KeyCode::ArrowDown, KeyCode::KeyS]),
        activate: kb.just_pressed_any(&[KeyCode::Enter, KeyCode::Space]) || left_click,
        raise: kb.just_pressed_any(&[KeyCode::ArrowRight, KeyCode::KeyD]),
        lower: kb.just_pressed_any(&[KeyCode::ArrowLeft, KeyCode::KeyA]) || right_click,
        back: kb.just_pressed(KeyCode::Backspace),
        pause: kb.just_pressed(KeyCode::KeyP),
        reset: kb.just_pressed(KeyCode::KeyR),
    }
}

/// Moves the menu cursor and carries out whatever the player chose.
fn handle_menu(engine: &mut Engine, gs: &mut GameState) {
    let input = read_menu_input(engine, gs);
    let items = gs.page.items();
    if input.up {
        gs.selected = (gs.selected + items.len() - 1) % items.len();
    }
    if input.down {
        gs.selected = (gs.selected + 1) % items.len();
    }
    let item = items[gs.selected];

    if gs.mode == Mode::Paused && (input.pause || (input.back && gs.page == Page::Pause)) {
        gs.mode = Mode::Playing;
    } else if gs.mode == Mode::Paused && input.reset {
        choose(engine, gs, Item::MainMenu);
    } else if input.back && gs.page != gs.home_page() {
        gs.open_page(gs.home_page());
    } else if let Item::Setting(setting) = item {
        // Enter and clicks only go up, so they wrap round from the top level to the bottom one.
        let level = gs.levels[setting];
        let new_level = if input.activate {
            (level + 1) % LEVELS
        } else if input.raise {
            (level + 1).min(LEVELS - 1)
        } else if input.lower {
            level.saturating_sub(1)
        } else {
            level
        };
        if new_level != level {
            gs.levels[setting] = new_level;
            match setting {
                MUSIC_SETTING => apply_music_volume(engine, gs),
                // A ding at the new volume, so the player can hear what they picked.
                SOUND_SETTING => play_sound(engine, gs, PIPE_SOUND, PIPE_SOUND_VOLUME),
                _ => {}
            }
            save_progress(gs);
        }
    } else if input.activate {
        choose(engine, gs, item);
    }
}

/// Carries out a menu item (settings are changed in `handle_menu`).
fn choose(engine: &mut Engine, gs: &mut GameState, item: Item) {
    match item {
        Item::Play => start_run(engine, gs),
        Item::Resume => gs.mode = Mode::Playing,
        Item::Restart => {
            finish_run(gs);
            start_run(engine, gs);
        }
        Item::Options => gs.open_page(Page::Options),
        Item::HowToPlay => gs.open_page(Page::HowToPlay),
        Item::MainMenu => {
            finish_run(gs);
            reset_run(engine, gs);
            gs.mode = Mode::Menu;
            gs.open_page(Page::Main);
        }
        Item::Quit => engine.should_exit = true,
        Item::Back => gs.open_page(gs.home_page()),
        Item::Setting(_) => {}
    }
}

fn set_text(engine: &mut Engine, label: &str, value: impl Into<String>) {
    if let Some(text) = engine.texts.get_mut(label) {
        text.value = value.into();
    }
}

/// Puts a line of menu text in place.
fn put_text(
    engine: &mut Engine,
    label: &str,
    value: impl Into<String>,
    y: f32,
    font_size: f32,
    scale: f32,
) {
    if let Some(text) = engine.texts.get_mut(label) {
        text.value = value.into();
        text.translation = Vec2::new(0.0, y);
        text.font_size = font_size;
        text.scale = scale;
    }
}

/// The words for a menu item, with arrows round it when the cursor is on it.
fn item_text(item: Item, gs: &GameState, selected: bool) -> String {
    let name = match item {
        Item::Play => "Play",
        Item::Resume => "Resume",
        Item::Restart => "Restart",
        Item::Options => "Options",
        Item::HowToPlay => "How to Play",
        Item::MainMenu => "Main Menu",
        Item::Quit => "Quit",
        Item::Back => "Back",
        Item::Setting(s) => {
            let mut level = LEVEL_NAMES[s][gs.levels[s]].to_string();
            if s == DIFFICULTY {
                level = format!(
                    "{} (points x{})",
                    level, DIFFICULTY_MULTIPLIER[gs.levels[s]]
                );
            }
            return if selected {
                format!(">  {}:  < {} >  <", SETTING_NAMES[s], level)
            } else {
                format!("{}:  {}", SETTING_NAMES[s], level)
            };
        }
    };
    if selected {
        format!(">  {}  <", name)
    } else {
        name.to_string()
    }
}

const HOW_TO_PLAY: [&str; 5] = [
    "Space / left click: flap your wings",
    "G / right click: glide towards the mouse",
    "Gliding down builds speed; climbing bleeds it off",
    "Too slow while climbing and you stall and drop",
    "Score faster near the front and while flying fast",
];

/// Draws the score during a run, and the menu page while in the menu or paused.
fn update_ui(engine: &mut Engine, gs: &GameState) {
    let showing = matches!(gs.mode, Mode::Menu | Mode::Paused);
    if let Some(panel) = engine.sprites.get_mut("menu_panel") {
        panel.translation = if showing { Vec2::ZERO } else { OFFSCREEN };
    }

    let (score_line, pipes_line) = if matches!(gs.mode, Mode::Playing | Mode::Dying) {
        let bird_x = engine.sprites["user"].translation.x;
        (
            format!("Score: {}", gs.points()),
            format!(
                "Pipes: {}     Multiplier: x{:.1}",
                gs.pipes,
                gs.score_multiplier(bird_x)
            ),
        )
    } else {
        (String::new(), String::new())
    };
    set_text(engine, "hud_score", score_line);
    set_text(engine, "hud_pipes", pipes_line);
    update_speed_bar(engine, gs);

    if !showing {
        for label in [MENU_TITLE, MENU_SUBTITLE, MENU_INFO, MENU_HINT]
            .into_iter()
            .chain(MENU_LINES)
        {
            set_text(engine, label, "");
        }
        return;
    }

    let page = gs.page;
    let time = engine.time_since_startup_f64 as f32;

    // The start menu's title bobs gently, like the bird.
    let bob = if page == Page::Main {
        6.0 * (time * 2.0).sin()
    } else {
        0.0
    };
    let (title, title_size) = match page {
        Page::Main => ("FlutterWorld", 88.0),
        Page::Pause => ("Paused", 72.0),
        Page::Options => ("Options", 64.0),
        Page::HowToPlay => ("How to Play", 64.0),
    };
    put_text(engine, MENU_TITLE, title, 205.0 + bob, title_size, 1.0);

    let subtitle = match page {
        Page::Main => "Fly through the gaps. Don't touch the pillars!".to_string(),
        Page::Pause => format!("Score: {}     Pipes: {}", gs.points(), gs.pipes),
        Page::Options => "Changes are saved automatically".to_string(),
        Page::HowToPlay => String::new(),
    };
    put_text(engine, MENU_SUBTITLE, subtitle, 140.0, 26.0, 1.0);

    let info = match (page, gs.last_run) {
        (Page::Main, Some((points, pipes))) => {
            format!(
                "Best: {}     Last run: {} points, {} pipes",
                gs.high_score, points, pipes
            )
        }
        (Page::Main, None) if gs.high_score > 0 => format!("Best: {}", gs.high_score),
        _ => String::new(),
    };
    put_text(engine, MENU_INFO, info, 95.0, 24.0, 1.0);

    // (text, y, font size, scale) of each line: How to Play's instructions, then the page's items.
    let mut lines: Vec<(String, f32, f32, f32)> = Vec::new();
    if page == Page::HowToPlay {
        let pipe_points = (PIPE_POINTS * gs.difficulty_multiplier()) as u32;
        let pipe_line = format!("Each pipe: +{} points and a bigger multiplier", pipe_points);
        for (i, line) in HOW_TO_PLAY
            .map(String::from)
            .into_iter()
            .chain([pipe_line])
            .enumerate()
        {
            lines.push((line, 140.0 - 42.0 * i as f32, 24.0, 1.0));
        }
    }
    // The item under the cursor gently pulses.
    let pulse = 1.0 + 0.03 * (time * 6.0).sin();
    for (i, &item) in page.items().iter().enumerate() {
        let selected = i == gs.selected;
        let font_size = match (item, selected) {
            (Item::Setting(_), false) => SETTING_FONT_SIZE,
            (Item::Setting(_), true) => SELECTED_SETTING_FONT_SIZE,
            (_, false) => ITEM_FONT_SIZE,
            (_, true) => SELECTED_FONT_SIZE,
        };
        let scale = if selected { pulse } else { 1.0 };
        lines.push((
            item_text(item, gs, selected),
            page.item_y(i),
            font_size,
            scale,
        ));
    }
    let mut lines = lines.into_iter();
    for label in MENU_LINES {
        match lines.next() {
            Some((text, y, font_size, scale)) => put_text(engine, label, text, y, font_size, scale),
            None => set_text(engine, label, ""),
        }
    }

    let hint = match page {
        Page::Main => "Up / Down or mouse: choose     Enter or click: select     Esc: quit",
        Page::Pause => "P: resume     R: main menu     Enter or click: select",
        Page::Options => "Left / Right or left / right click: change     Backspace: back",
        Page::HowToPlay => "In a run:   P: pause     R: back to the menu     Esc: quit",
    };
    put_text(engine, MENU_HINT, hint, -235.0, 20.0, 1.0);
}

/// Where a glide speed sits along the speed bar, as an x position.
fn speed_bar_x(speed: f32) -> f32 {
    let t = ((speed - GLIDE_MIN_SPEED) / (GLIDE_MAX_SPEED - GLIDE_MIN_SPEED)).clamp(0.0, 1.0);
    -SPEED_BAR_W / 2.0 + t * SPEED_BAR_W
}

/// The fixed-size pieces of the speed bar: (label, size, colour). The fill is drawn separately.
fn speed_shapes() -> [(&'static str, Vec2, Color); 5] {
    let stall_w = speed_bar_x(GLIDE_STALL_SPEED) + SPEED_BAR_W / 2.0;
    [
        (SPEED_BACK, SPEED_BACK_SIZE, SPEED_BACK_COLOR),
        (
            SPEED_TRACK,
            Vec2::new(SPEED_BAR_W, SPEED_BAR_H),
            SPEED_TRACK_COLOR,
        ),
        (
            SPEED_STALL_ZONE,
            Vec2::new(stall_w, SPEED_BAR_H),
            SPEED_STALL_ZONE_COLOR,
        ),
        (SPEED_RECOVER, SPEED_RECOVER_SIZE, SPEED_RECOVER_COLOR),
        (SPEED_NEEDLE, SPEED_NEEDLE_SIZE, Color::WHITE),
    ]
}

/// The label of the fill segment of the given colour that is 2^`segment` pixels wide.
fn fill_label(color: usize, segment: u32) -> String {
    format!("{SPEED_FILL}_{color}_{segment}")
}

/// Which layer a piece of the speed bar is drawn on, going by SPEED_PIECES.
fn speed_layer(piece: &str) -> f32 {
    let index = SPEED_PIECES
        .iter()
        .position(|&label| label == piece)
        .unwrap_or(0);
    SPEED_HUD + index as f32
}

/// Draws the speed bar during a run, and hides it otherwise. While stalled the bar turns red, a
/// "Recover" mark shows the speed the falling bird must reach to steer again, and a warning flashes.
fn update_speed_bar(engine: &mut Engine, gs: &GameState) {
    let showing = matches!(gs.mode, Mode::Playing | Mode::Dying);
    let speed = gs.glide_speed;
    let left = -SPEED_BAR_W / 2.0;
    let fill_w = speed_bar_x(speed) - left;
    let stall_x = speed_bar_x(GLIDE_STALL_SPEED);
    // index into SPEED_FILL_COLORS
    let fill_color = if gs.stalled {
        2
    } else if speed < GLIDE_BASE_SPEED {
        1
    } else {
        0
    };

    // (label, centre, whether it's shown)
    let pieces = [
        (SPEED_BACK, SPEED_BACK_CENTER, true),
        (SPEED_TRACK, Vec2::new(0.0, SPEED_BAR_Y), true),
        (
            SPEED_STALL_ZONE,
            Vec2::new((left + stall_x) / 2.0, SPEED_BAR_Y),
            true,
        ),
        (
            SPEED_RECOVER,
            Vec2::new(speed_bar_x(GLIDE_RECOVER_SPEED), SPEED_BAR_Y),
            gs.stalled,
        ),
        (
            SPEED_NEEDLE,
            Vec2::new(speed_bar_x(speed), SPEED_BAR_Y),
            true,
        ),
    ];
    for (label, center, shown) in pieces {
        if let Some(piece) = engine.sprites.get_mut(label) {
            piece.translation = if showing && shown { center } else { OFFSCREEN };
        }
    }

    // Lay the fill segments end to end from the left, widest first, using the ones whose bits make
    // up the fill's width in pixels.
    let fill_px = fill_w.round() as u32;
    let mut x = left;
    for segment in (0..SPEED_FILL_SEGMENTS).rev() {
        let width = (1u32 << segment) as f32;
        let used = fill_px & (1 << segment) != 0;
        for color in 0..SPEED_FILL_COLORS.len() {
            if let Some(piece) = engine.sprites.get_mut(&fill_label(color, segment)) {
                let shown = showing && used && color == fill_color;
                piece.translation = if shown {
                    Vec2::new(x + width / 2.0, SPEED_BAR_Y)
                } else {
                    OFFSCREEN
                };
            }
        }
        if used {
            x += width;
        }
    }

    let blink_on = (engine.time_since_startup_f64 * 4.0) as u64 % 2 == 0;
    let title = match (showing, gs.stalled) {
        (false, _) => String::new(),
        (true, true) if blink_on => "STALL!  Dive to recover".to_string(),
        (true, true) => String::new(),
        (true, false) => format!("Speed {:.1}", speed),
    };
    let label = |text: String| if showing { text } else { String::new() };
    set_text(engine, SPEED_TITLE, title);
    set_text(
        engine,
        SPEED_MIN_LABEL,
        label(format!("Min {}", GLIDE_MIN_SPEED)),
    );
    set_text(
        engine,
        SPEED_MAX_LABEL,
        label(format!("Max {}", GLIDE_MAX_SPEED)),
    );
    set_text(engine, SPEED_STALL_LABEL, label("Stall".to_string()));
    let recover = if gs.stalled {
        "Recover".to_string()
    } else {
        String::new()
    };
    set_text(engine, SPEED_RECOVER_LABEL, label(recover));
}

fn game_logic(engine: &mut Engine, gs: &mut GameState) {
    // Only collisions involving the bird matter; pillars overlapping the floor are just scenery.
    let bird_hit = engine
        .collision_events
        .drain(..)
        .any(|event| event.state == CollisionState::Begin && event.pair.either_contains("user"));

    // Keys and menus are handled once per frame; everything that moves advances in fixed steps,
    // so the game runs at the same speed on any monitor.
    let steps = gs.clock.steps(engine.delta_f32);

    match gs.mode {
        Mode::Menu => {
            handle_menu(engine, gs);
            if gs.mode == Mode::Menu {
                if bird_hit {
                    // The AI crashed: just start its run over.
                    reset_run(engine, gs);
                }
                for _ in 0..steps {
                    scroll_world(engine, gs);
                    let pairs = read_pairs(engine);
                    let player = engine.sprites.get_mut("user").unwrap();
                    let controls = ai_controls(player.translation, gs.prev_flap, &pairs);
                    step_bird(&mut player.translation, &mut player.rotation, gs, &controls);
                }
            }
        }
        Mode::Playing => {
            let pause = engine.keyboard_state.just_pressed(KeyCode::KeyP);
            let reset = engine.keyboard_state.just_pressed(KeyCode::KeyR);
            if pause {
                gs.mode = Mode::Paused;
                gs.open_page(Page::Pause);
            } else if reset {
                choose(engine, gs, Item::MainMenu);
            } else if bird_hit {
                println!("Oh no! Flutter died! Try again next time!");
                play_sound(engine, gs, DEATH_SOUND, DEATH_SOUND_VOLUME);
                engine.audio_manager.stop_music();
                finish_run(gs);
                gs.start_dying();
            } else {
                let controls = player_controls(engine);
                for _ in 0..steps {
                    scroll_world(engine, gs);
                    let player = engine.sprites.get_mut("user").unwrap();
                    step_bird(&mut player.translation, &mut player.rotation, gs, &controls);
                    award_points(engine, gs);
                }
            }
        }
        Mode::Paused => handle_menu(engine, gs),
        Mode::Dying => {
            // The world stands still while the bird falls, and the menu returns afterwards.
            for _ in 0..steps {
                let player = engine.sprites.get_mut("user").unwrap();
                if step_death(&mut player.translation, &mut player.rotation, gs) {
                    reset_run(engine, gs);
                    gs.mode = Mode::Menu;
                    gs.open_page(Page::Main);
                    apply_music_volume(engine, gs);
                    break;
                }
            }
        }
    }

    update_ui(engine, gs);
}

/// Adds the dark panel, the text lines the menu pages are drawn with, and the score display.
fn add_ui(engine: &mut Engine) {
    let panel = engine.add_sprite("menu_panel", "sprite/flutter/menu_panel.png");
    panel.layer = PANEL;
    panel.translation = OFFSCREEN;
    for label in [MENU_TITLE, MENU_SUBTITLE, MENU_INFO, MENU_HINT]
        .into_iter()
        .chain(MENU_LINES)
    {
        let text = engine.add_text(label, "");
        text.font = MENU_FONT.to_string();
    }
    for (label, y, font_size) in HUD_TEXTS {
        let text = engine.add_text(label, "");
        text.font = MENU_FONT.to_string();
        text.font_size = font_size;
        text.translation = Vec2::new(0.0, y);
    }

    for (label, size, color) in speed_shapes() {
        let piece = engine.add_sprite(label, rect_image(size, color));
        piece.layer = speed_layer(label);
        piece.translation = OFFSCREEN;
    }
    for (color, fill) in SPEED_FILL_COLORS.into_iter().enumerate() {
        for segment in 0..SPEED_FILL_SEGMENTS {
            let size = Vec2::new((1u32 << segment) as f32, SPEED_BAR_H);
            let piece = engine.add_sprite(fill_label(color, segment), rect_image(size, fill));
            piece.layer = speed_layer(SPEED_FILL);
            piece.translation = OFFSCREEN;
        }
    }
    let bar_end = SPEED_BAR_W / 2.0 + 48.0;
    let speed_texts = [
        (SPEED_TITLE, Vec2::new(0.0, -336.0), 20.0),
        (SPEED_MIN_LABEL, Vec2::new(-bar_end, SPEED_BAR_Y), 18.0),
        (SPEED_MAX_LABEL, Vec2::new(bar_end, SPEED_BAR_Y), 18.0),
        (
            SPEED_STALL_LABEL,
            Vec2::new(
                (speed_bar_x(GLIDE_MIN_SPEED) + speed_bar_x(GLIDE_STALL_SPEED)) / 2.0,
                SPEED_LABEL_Y,
            ),
            16.0,
        ),
        (
            SPEED_RECOVER_LABEL,
            Vec2::new(speed_bar_x(GLIDE_RECOVER_SPEED), SPEED_LABEL_Y),
            16.0,
        ),
    ];
    for (label, position, font_size) in speed_texts {
        let text = engine.add_text(label, "");
        text.font = MENU_FONT.to_string();
        text.font_size = font_size;
        text.translation = position;
    }
}

/// Makes a solid-coloured image of the given size (once, under assets/) and returns its path for
/// `add_sprite`. Must run before the game starts, so the engine finds the file when it loads it.
fn rect_image(size: Vec2, color: Color) -> String {
    let (width, height) = (
        size.x.round().max(1.0) as u32,
        size.y.round().max(1.0) as u32,
    );
    let srgba = color.to_srgba();
    let rgba = [srgba.red, srgba.green, srgba.blue, srgba.alpha]
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    let path = format!(
        "{GENERATED_IMAGES}/rect_{width}x{height}_{:02x}{:02x}{:02x}{:02x}.png",
        rgba[0], rgba[1], rgba[2], rgba[3]
    );
    let file = asset_root().join(&path);
    if !file.exists() {
        let written = file
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| std::fs::write(&file, solid_png(width, height, rgba)));
        if let Err(err) = written {
            eprintln!("Couldn't write {}: {}", file.display(), err);
        }
    }
    path
}

/// The assets folder the engine loads from: the project folder under `cargo run`, otherwise the
/// folder the executable is in (the same places Bevy looks).
fn asset_root() -> PathBuf {
    let base = std::env::var_os("BEVY_ASSET_ROOT")
        .or_else(|| std::env::var_os("CARGO_MANIFEST_DIR"))
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok()?.parent().map(PathBuf::from))
        .unwrap_or_default();
    base.join("assets")
}

/// A PNG file of one colour. The pixels are stored uncompressed, which is fine for small images.
fn solid_png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut pixels = Vec::new();
    for _ in 0..height {
        pixels.push(0); // no filter on this row
        for _ in 0..width {
            pixels.extend_from_slice(&rgba);
        }
    }
    // A zlib stream made of "stored" deflate blocks, which hold at most 65535 bytes each.
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = pixels.chunks(65535).collect();
    for (i, block) in blocks.iter().enumerate() {
        let len = block.len() as u16;
        zlib.push((i + 1 == blocks.len()) as u8);
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&pixels).to_be_bytes());

    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bits per channel, RGBA
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png_chunk(&mut png, b"IHDR", &header);
    png_chunk(&mut png, b"IDAT", &zlib);
    png_chunk(&mut png, b"IEND", &[]);
    png
}

fn png_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = png.len();
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    let crc = crc32(&png[start..]);
    png.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in bytes {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// Where the best score and the settings are kept between games.
fn save_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(base).join("FlutterWorld").join("save.txt"))
}

fn save_text(gs: &GameState) -> String {
    let levels: Vec<String> = gs.levels.iter().map(|level| level.to_string()).collect();
    format!("best={}\nlevels={}\n", gs.high_score, levels.join(","))
}

/// Reads back what `save_text` wrote, ignoring anything it doesn't recognise.
fn read_save_text(text: &str, gs: &mut GameState) {
    for line in text.lines() {
        match line.split_once('=') {
            Some(("best", value)) => {
                if let Ok(best) = value.trim().parse() {
                    gs.high_score = best;
                }
            }
            Some(("levels", value)) => {
                for (level, saved) in gs.levels.iter_mut().zip(value.split(',')) {
                    if let Ok(saved) = saved.trim().parse::<usize>() {
                        if saved < LEVELS {
                            *level = saved;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn load_progress(gs: &mut GameState) {
    if let Some(text) = save_path().and_then(|path| std::fs::read_to_string(path).ok()) {
        read_save_text(&text, gs);
    }
}

fn save_progress(gs: &GameState) {
    let Some(path) = save_path() else { return };
    let saved = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|_| std::fs::write(&path, save_text(gs)));
    if let Err(err) = saved {
        eprintln!("Couldn't save to {}: {}", path.display(), err);
    }
}

fn main() {
    let mut game = Game::new();

    game.window_settings(Window {
        title: "FlutterWorld".into(),
        resolution: WindowResolution::new(WINDOW_WIDTH, WINDOW_HEIGHT),
        ..Default::default()
    }); //Makes the window named "FlutterWorld" instead of Rusty Engine

    let player = game.add_sprite("user", "sprite/flutter/avatar.png");
    player.translation = Vec2::new(PLAYER_START_X, 0.0);
    player.scale = 0.1;
    player.layer = USER;
    player.collision = true;

    // (labels, image, y, layer, whether the bird can crash into it)
    let tiles = [
        (
            BACKGROUND_LABELS,
            "sprite/flutter/background.png",
            0.0,
            BACKGROUND,
            false,
        ),
        (
            FLOOR_LABELS,
            "sprite/flutter/bg_floor.png",
            FLOOR_Y,
            FLOOR,
            true,
        ),
    ];
    for (labels, image, y, layer, collision) in tiles {
        for (i, label) in labels.into_iter().enumerate() {
            let tile = game.add_sprite(label, image);
            tile.translation = Vec2::new(i as f32 * TILE_WIDTH, y);
            tile.scale = TILE_SCALE;
            tile.layer = layer;
            tile.collision = collision;
        }
    }

    for i in 0..NUM_PILLARS {
        for top in [false, true] {
            let pillar = game.add_sprite(pillar_label(i, top), PILLAR_IMAGE);
            pillar.scale = PILLAR_SCALE;
            pillar.rotation = if top { PI } else { 0.0 };
            pillar.layer = PILLAR;
            pillar.collision = true;
        }
    }

    add_ui(&mut game);

    // The game opens on the menu, with the pillars laid out and the AI ready to fly.
    let mut game_state = GameState::new();
    load_progress(&mut game_state);
    reset_run(&mut game, &mut game_state);
    update_ui(&mut game, &game_state);
    apply_music_volume(&mut game, &game_state);
    // Always show the same area of the world, however big the window or monitor is.
    game.add_logic(window_to_world);
    game.add_logic(game_logic);
    game.add_logic(world_to_window);
    game.run(game_state); //runs the game in the specified game state
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the bird overlaps a pillar or the floor, using the same extents as the colliders.
    fn crashed(bird: Vec2, pairs: &[Pair]) -> bool {
        if bird.y - BIRD_HALF_H < FLOOR_TOP_Y {
            return true;
        }
        pairs.iter().any(|p| {
            (bird.x - p.x).abs() < PILLAR_HALF_W + BIRD_HALF_W
                && (bird.y - BIRD_HALF_H < p.e_b || bird.y + BIRD_HALF_H > p.e_t)
        })
    }

    /// Runs the AI for `frames` frames at the given settings; returns the frame it crashed on.
    fn simulate(levels: [usize; SETTING_COUNT], frames: usize) -> Option<usize> {
        let mut gs = GameState::new();
        gs.levels = levels;
        let (spacing, gap) = (gs.pillar_spacing(), gs.gap_size());
        let mut xs = [0.0; NUM_PILLARS];
        let mut pairs = [Pair {
            x: 0.0,
            e_b: 0.0,
            e_t: 0.0,
        }; NUM_PILLARS];
        for i in 0..NUM_PILLARS {
            xs[i] = FIRST_PILLAR_X + i as f32 * spacing;
            pairs[i] = pair_for(i, xs[i], gap);
        }
        let mut bird = Vec2::new(PLAYER_START_X, 0.0);
        let mut rotation = 0.0;
        for frame in 0..frames {
            let recycled = scroll_pillars(&mut xs, BASE_SCROLL_SPEED * gs.speed_scale(), spacing);
            for i in 0..NUM_PILLARS {
                pairs[i] = if recycled[i] {
                    pair_for(i, xs[i], gap)
                } else {
                    Pair {
                        x: xs[i],
                        ..pairs[i]
                    }
                };
            }
            let controls = ai_controls(bird, gs.prev_flap, &pairs);
            step_bird(&mut bird, &mut rotation, &mut gs, &controls);
            if crashed(bird, &pairs) {
                return Some(frame);
            }
        }
        None
    }

    #[test]
    fn ai_survives_every_setting_combination() {
        for f in 0..LEVELS {
            for g in 0..LEVELS {
                for s in 0..LEVELS {
                    let mut levels = [DEFAULT_LEVEL; SETTING_COUNT];
                    levels[FREQUENCY] = f;
                    levels[GAP] = g;
                    levels[DIFFICULTY] = s;
                    assert_eq!(
                        simulate(levels, 20_000),
                        None,
                        "AI crashed with frequency {}, gap {}, difficulty {}",
                        LEVEL_NAMES[FREQUENCY][f],
                        LEVEL_NAMES[GAP][g],
                        LEVEL_NAMES[DIFFICULTY][s]
                    );
                }
            }
        }
    }

    #[test]
    fn a_bird_that_never_flaps_crashes() {
        // Guards the AI test above: the crash check must be able to fail.
        let mut gs = GameState::new();
        let mut bird = Vec2::new(PLAYER_START_X, 0.0);
        let mut rotation = 0.0;
        let idle = Controls {
            flap: false,
            glide: false,
            glide_target: None,
        };
        let mut crashed_at = None;
        for frame in 0..200 {
            step_bird(&mut bird, &mut rotation, &mut gs, &idle);
            if crashed(bird, &[]) {
                crashed_at = Some(frame);
                break;
            }
        }
        assert!(
            crashed_at.is_some(),
            "an idle bird should fall into the floor"
        );
    }

    #[test]
    fn game_speed_is_the_same_at_any_refresh_rate() {
        for hz in [30.0, 60.0, 75.0, 120.0, 144.0, 165.0, 240.0] {
            let mut clock = StepClock::default();
            let steps: u32 = (0..(hz as usize * 10)).map(|_| clock.steps(1.0 / hz)).sum();
            assert!(
                steps.abs_diff(600) <= 1,
                "{hz} Hz ran {steps} steps in 10 seconds instead of 600"
            );
        }
    }

    #[test]
    fn jittery_60hz_frames_take_exactly_one_step_each() {
        let mut clock = StepClock::default();
        for frame in 0..600 {
            let jitter = if frame % 2 == 0 { 0.0006 } else { -0.0006 };
            assert_eq!(
                clock.steps(1.0 / 60.0 + jitter),
                1,
                "frame {frame} skipped or doubled a step"
            );
        }
    }

    #[test]
    fn a_long_hitch_only_catches_up_a_little() {
        let mut clock = StepClock::default();
        assert_eq!(clock.steps(2.0), MAX_STEPS_PER_FRAME);
        assert_eq!(clock.steps(1.0 / 60.0), 1);
    }

    #[test]
    fn every_menu_item_is_on_the_panel_and_can_be_pointed_at() {
        for page in [Page::Main, Page::Pause, Page::Options, Page::HowToPlay] {
            let items = page.items().len();
            let info_lines = if page == Page::HowToPlay {
                HOW_TO_PLAY.len() + 1
            } else {
                0
            };
            assert!(
                info_lines + items <= MENU_LINES.len(),
                "{page:?} has more lines than there is text for"
            );
            for i in 0..items {
                let y = page.item_y(i);
                assert!(y.abs() < 280.0, "{page:?} item {i} is off the panel");
                assert_eq!(page.item_at(Vec2::new(100.0, y)), Some(i));
                assert_eq!(page.item_at(Vec2::new(PANEL_HALF_W + 1.0, y)), None);
            }
        }
    }

    #[test]
    fn loud_sound_settings_stay_within_full_volume() {
        let loudest = SOUND_VOLUMES[LEVELS - 1];
        assert!(PIPE_SOUND_VOLUME * loudest <= 1.0 && DEATH_SOUND_VOLUME * loudest <= 1.0);
    }

    #[test]
    fn saved_progress_reads_back_the_same() {
        let mut gs = GameState::new();
        gs.high_score = 4321;
        gs.levels = [0, 1, 2, 3, 4];
        let mut loaded = GameState::new();
        read_save_text(&save_text(&gs), &mut loaded);
        assert_eq!(loaded.high_score, 4321);
        assert_eq!(loaded.levels, [0, 1, 2, 3, 4]);

        // A damaged save keeps the defaults rather than crashing or picking a level that doesn't exist.
        let mut damaged = GameState::new();
        read_save_text("best=lots\nlevels=9,x,1\nnonsense", &mut damaged);
        assert_eq!(damaged.high_score, 0);
        assert_eq!(
            damaged.levels,
            [
                DEFAULT_LEVEL,
                DEFAULT_LEVEL,
                1,
                DEFAULT_LEVEL,
                DEFAULT_LEVEL
            ]
        );
    }

    #[test]
    fn death_animation_shakes_then_drops_the_bird_out_of_view() {
        // Crashing at the very top of the window is the longest way down.
        let mut gs = GameState::new();
        gs.start_dying();
        let crash_at = Vec2::new(0.0, PLAYER_MAX_Y);
        let mut bird = crash_at;
        let mut rotation = 0.0;
        for _ in 0..DEATH_FREEZE_FRAMES {
            assert!(!step_death(&mut bird, &mut rotation, &mut gs));
        }
        assert_eq!(
            bird, crash_at,
            "the shake should end where the bird crashed"
        );

        let mut frames = DEATH_FREEZE_FRAMES + 1;
        while !step_death(&mut bird, &mut rotation, &mut gs) {
            frames += 1;
        }
        assert_eq!(frames, DEATH_FRAMES);
        assert!(
            bird.y <= DEATH_OFFSCREEN_Y,
            "the bird was still in view when the menu came back"
        );
    }

    #[test]
    fn pillars_always_cover_floor_and_ceiling() {
        for level in 0..LEVELS {
            let gap = BASE_GAP * GAP_SCALE[level];
            for i in 0..NUM_PILLARS {
                let pair = pair_for(i, 0.0, gap);
                assert!(
                    pair.e_b - PILLAR_LENGTH <= FLOOR_HIDE_Y,
                    "bottom pillar floats at gap level {level}"
                );
                assert!(
                    pair.e_t + PILLAR_LENGTH >= CEILING_COVER_Y,
                    "top pillar stops short at gap level {level}"
                );
                assert!((pair.e_t - pair.e_b - gap).abs() < 0.01);
            }
        }
    }

    #[test]
    fn default_settings_match_the_original_game() {
        let gs = GameState::new();
        assert_eq!(gs.pillar_spacing(), 650.0);
        assert_eq!(gs.speed_scale(), 1.0);
        assert_eq!(gs.gap_size(), BASE_GAP);
    }

    #[test]
    fn points_multiplier_grows_with_front_speed_difficulty_and_pipes() {
        let mut gs = GameState::new();
        let base = gs.score_multiplier(PLAYER_MIN_X);
        assert!(
            (base - 1.0).abs() < 1e-6,
            "back of the window at normal settings is x1, got {base}"
        );

        assert!(gs.score_multiplier(PLAYER_MAX_X) > gs.score_multiplier(0.0));
        assert!((gs.score_multiplier(PLAYER_MAX_X) - (1.0 + FRONT_BONUS)).abs() < 1e-6);

        gs.glide_speed = GLIDE_MAX_SPEED;
        assert!(
            gs.score_multiplier(PLAYER_MIN_X) > base,
            "gliding fast should score more"
        );
        gs.glide_speed = GLIDE_MIN_SPEED;
        assert_eq!(
            gs.score_multiplier(PLAYER_MIN_X),
            base,
            "slow flight shouldn't cost points"
        );
        gs.glide_speed = GLIDE_BASE_SPEED;

        gs.levels[DIFFICULTY] = LEVELS - 1;
        assert!(
            gs.score_multiplier(PLAYER_MIN_X) > base,
            "harder difficulty should score more"
        );
        gs.levels[DIFFICULTY] = DEFAULT_LEVEL;

        gs.pipes = 10;
        assert!(
            gs.score_multiplier(PLAYER_MIN_X) > base,
            "more pipes should score more"
        );
    }

    #[test]
    fn speed_bar_runs_from_min_to_max_with_stall_and_recovery_in_order() {
        assert_eq!(speed_bar_x(GLIDE_MIN_SPEED), -SPEED_BAR_W / 2.0);
        assert_eq!(speed_bar_x(GLIDE_MAX_SPEED), SPEED_BAR_W / 2.0);
        assert_eq!(
            speed_bar_x(0.0),
            speed_bar_x(GLIDE_MIN_SPEED),
            "the needle stays on the bar"
        );
        assert_eq!(
            speed_bar_x(100.0),
            speed_bar_x(GLIDE_MAX_SPEED),
            "the needle stays on the bar"
        );
        let marks = [
            GLIDE_MIN_SPEED,
            GLIDE_STALL_SPEED,
            GLIDE_BASE_SPEED,
            GLIDE_RECOVER_SPEED,
            GLIDE_MAX_SPEED,
        ];
        for pair in marks.windows(2) {
            assert!(
                speed_bar_x(pair[0]) < speed_bar_x(pair[1]),
                "{} should sit left of {}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn harder_difficulty_means_faster_pipes_and_more_points() {
        for level in 1..LEVELS {
            assert!(SPEED_SCALE[level] > SPEED_SCALE[level - 1]);
            assert!(DIFFICULTY_MULTIPLIER[level] > DIFFICULTY_MULTIPLIER[level - 1]);
        }
    }

    #[test]
    fn each_pipe_is_counted_once_and_only_when_fully_behind_the_bird() {
        let bird_x = 0.0;
        let mut xs = [1000.0; NUM_PILLARS];
        let mut passed = [false; NUM_PILLARS];
        assert_eq!(count_passed(&xs, &mut passed, bird_x), 0);

        // Still overlapping the bird: not passed yet.
        xs[0] = -(PILLAR_HALF_W + BIRD_HALF_W) + 1.0;
        assert_eq!(count_passed(&xs, &mut passed, bird_x), 0);

        // Fully behind it: counts once, never again.
        xs[0] = -(PILLAR_HALF_W + BIRD_HALF_W) - 1.0;
        xs[1] = -500.0;
        assert_eq!(count_passed(&xs, &mut passed, bird_x), 2);
        assert_eq!(count_passed(&xs, &mut passed, bird_x), 0);

        // The bird drifting back in front of a pillar it already cleared doesn't count it again.
        assert_eq!(count_passed(&xs, &mut passed, -600.0), 0);
        assert_eq!(count_passed(&xs, &mut passed, bird_x), 0);
    }

    #[test]
    fn ending_a_run_records_it_and_keeps_the_best() {
        let mut gs = GameState::new();
        gs.score = 120.9;
        gs.pipes = 3;
        gs.end_run();
        assert_eq!(gs.last_run, Some((120, 3)));
        assert_eq!(gs.high_score, 120);

        gs.reset_score();
        gs.score = 40.0;
        gs.end_run();
        assert_eq!(gs.last_run, Some((40, 0)));
        assert_eq!(gs.high_score, 120);
    }

    #[test]
    fn turn_toward_takes_the_short_way_round() {
        // From just under +PI to just over -PI is a small turn through PI, not a lap back through 0.
        let turned = turn_toward(3.0, -3.0, 0.2);
        assert!(
            (turned - (3.2 - 2.0 * PI)).abs() < 1e-5,
            "turned the long way: {turned}"
        );
        assert!((turn_toward(0.0, 1.0, 0.25) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn fill_segments_cover_the_whole_bar() {
        assert!(SPEED_BAR_W.round() as u32 <= (1 << SPEED_FILL_SEGMENTS) - 1);
    }

    #[test]
    fn png_checksums_match_known_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn solid_png_spans_several_deflate_blocks() {
        // 600 x 84 RGBA is over 65535 bytes, so it needs more than one stored block.
        let png = solid_png(600, 84, [0, 0, 0, 140]);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }
}
