use rusty_engine::{game, prelude::{bevy::input::keyboard::Key, *}};
use std::f32::consts::{FRAC_PI_2, PI};

/// Rotates `current` toward `target` by at most `max_step` radians, taking the shorter way
/// round, and keeps the result in (-PI, PI].
fn turn_toward(current: f32, target: f32, max_step: f32) -> f32 {
    let diff = (target - current + PI).rem_euclid(2.0 * PI) - PI;
    let turned = current + diff.clamp(-max_step, max_step);
    (turned + PI).rem_euclid(2.0 * PI) - PI
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Menu,    // start screen, with an AI flying in the background
    Playing,
    Paused,
}

#[derive(Resource)]
struct GameState {
    _current_score: u128,
    _high_score: u128,
    _health_left: u128,
    mode: Mode,
    levels: [usize; SETTING_COUNT], // chosen level of each setting, indexes into the tables below
    selected: usize,                // which setting row the menu cursor is on
    lift: f32,
    glide_speed: f32,
    stalled: bool,
    heading: f32, // flight direction in radians: 0 is forward, positive is up
    prev_flap: bool,
}

impl GameState {
    fn new() -> Self {
        GameState {
            _current_score: 0,
            _high_score: 0,
            _health_left: 1,
            mode: Mode::Menu,
            levels: [DEFAULT_LEVEL; SETTING_COUNT],
            selected: 0,
            lift: 0.0,
            glide_speed: GLIDE_BASE_SPEED,
            stalled: false,
            heading: 0.0,
            prev_flap: false,
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
        SPEED_SCALE[self.levels[SPEED]]
    }
}

// Menu settings. Each one has LEVELS steps; the middle step is the original game's value.
const LEVELS: usize = 5;
const DEFAULT_LEVEL: usize = 2;
const SETTING_COUNT: usize = 3;
const FREQUENCY: usize = 0;
const GAP: usize = 1;
const SPEED: usize = 2;
const SETTING_NAMES: [&str; SETTING_COUNT] = ["Pillar frequency", "Gap size", "Bird speed"];
const LEVEL_NAMES: [[&str; LEVELS]; SETTING_COUNT] = [
    ["Very rare", "Rare", "Normal", "Frequent", "Very frequent"],
    ["Tiny", "Small", "Normal", "Large", "Huge"],
    ["Very slow", "Slow", "Normal", "Fast", "Very fast"],
];
const PILLAR_SPACING: [f32; LEVELS] = [900.0, 750.0, 650.0, 550.0, 450.0]; // x distance between pillars
const GAP_SCALE: [f32; LEVELS] = [0.6, 0.8, 1.0, 1.2, 1.4];
const SPEED_SCALE: [f32; LEVELS] = [0.6, 0.8, 1.0, 1.25, 1.5]; // how fast the world scrolls past the bird
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

const MENU_FONT: &str = "font/BitcountPropSingle-VariableFont_CRSV,ELSH,ELXP,slnt,wght.ttf";
const OFFSCREEN: Vec2 = Vec2::new(-5000.0, 0.0);
// (label, y, font size) of every line of text on the menu and pause screens
const UI_TEXTS: [(&str, f32, f32); 7] = [
    ("ui_title", 215.0, 84.0),
    ("ui_subtitle", 130.0, 34.0),
    ("ui_row_0", 50.0, 32.0),
    ("ui_row_1", -15.0, 32.0),
    ("ui_row_2", -80.0, 32.0),
    ("ui_hint", -155.0, 24.0),
    ("ui_controls", -220.0, 24.0),
];

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
    Pair { x, e_b: center - gap / 2.0, e_t: center + gap / 2.0 }
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

/// Height the AI should fly at: the middle of the next gap it still has to get through.
fn ai_target(bird_x: f32, pairs: &[Pair]) -> f32 {
    let mut next: Option<&Pair> = None;
    for pair in pairs {
        let still_ahead = pair.x + PILLAR_HALF_W + BIRD_HALF_W + AI_CLEARANCE > bird_x;
        if still_ahead && next.map_or(true, |n| pair.x < n.x) {
            next = Some(pair);
        }
    }
    next.map_or(0.0, |p| (p.e_b + p.e_t) / 2.0).clamp(AI_MIN_Y, AI_MAX_Y)
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
    Controls { flap, glide: false, glide_target: None }
}

fn pillar_label(index: usize, top: bool) -> String {
    format!("pillar_{}_{}", index, if top { "top" } else { "bottom" })
}

/// Puts a pillar pair at `x` with a gap of the given size.
fn place_pillar_pair(engine: &mut Engine, index: usize, x: f32, gap: f32) {
    let pair = pair_for(index, x, gap);
    let bottom = engine.sprites.get_mut(pillar_label(index, false).as_str()).unwrap();
    bottom.translation = Vec2::new(x, pair.e_b - PILLAR_EDGE);
    let top = engine.sprites.get_mut(pillar_label(index, true).as_str()).unwrap();
    top.translation = Vec2::new(x, pair.e_t + PILLAR_EDGE);
}

fn read_pairs(engine: &Engine) -> [Pair; NUM_PILLARS] {
    let mut pairs = [Pair { x: 0.0, e_b: 0.0, e_t: 0.0 }; NUM_PILLARS];
    for (i, pair) in pairs.iter_mut().enumerate() {
        let bottom = &engine.sprites[pillar_label(i, false).as_str()];
        let top = &engine.sprites[pillar_label(i, true).as_str()];
        *pair = Pair {
            x: bottom.translation.x,
            e_b: bottom.translation.y + PILLAR_EDGE,
            e_t: top.translation.y - PILLAR_EDGE,
        };
    }
    pairs
}

/// Starts a run from scratch: the bird goes back to its starting spot and the pillars are laid
/// out again using the current settings.
fn reset_run(engine: &mut Engine, gs: &mut GameState) {
    let (spacing, gap) = (gs.pillar_spacing(), gs.gap_size());
    for i in 0..NUM_PILLARS {
        place_pillar_pair(engine, i, FIRST_PILLAR_X + i as f32 * spacing, gap);
    }
    gs.reset_bird();
    let player = engine.sprites.get_mut("user").unwrap();
    player.translation = Vec2::new(PLAYER_START_X, 0.0);
    player.rotation = 0.0;
}

/// Scrolls the floor, background and pillars. The speed setting scales all of them.
fn scroll_world(engine: &mut Engine, gs: &GameState) {
    let scale = gs.speed_scale();
    for label in ["floor", "floor2"] {
        engine.sprites.get_mut(label).unwrap().translation.x -= BASE_SCROLL_SPEED * scale;
    }
    for label in ["background", "background2"] {
        engine.sprites.get_mut(label).unwrap().translation.x -= BASE_BACKGROUND_SPEED * scale;
    }

    let bg_width = 1600.0 * 1.7;
    for bg_label in ["background", "background2"] {
        let bg_x = engine.sprites.get(bg_label).unwrap().translation.x;
        if bg_x <= -bg_width {
            let other_label = if bg_label == "background" { "background2" } else { "background" };
            let other_x = engine.sprites.get(other_label).unwrap().translation.x;
            engine.sprites.get_mut(bg_label).unwrap().translation.x = other_x + bg_width;
        }
    }

    let floor_width = 1600.0 * 1.7;
    for floor_label in ["floor", "floor2"] {
        let floor_x = engine.sprites.get(floor_label).unwrap().translation.x;
        if floor_x <= -floor_width {
            let other_label = if floor_label == "floor" { "floor2" } else { "floor" };
            let other_x = engine.sprites.get(other_label).unwrap().translation.x;
            engine.sprites.get_mut(floor_label).unwrap().translation.x = other_x + floor_width;
        }
    }

    let mut xs = [0.0; NUM_PILLARS];
    for (i, x) in xs.iter_mut().enumerate() {
        *x = engine.sprites[pillar_label(i, false).as_str()].translation.x;
    }
    let recycled = scroll_pillars(&mut xs, BASE_SCROLL_SPEED * scale, gs.pillar_spacing());
    for i in 0..NUM_PILLARS {
        if recycled[i] {
            // A recycled pair is brand new, so it picks up the current gap size.
            place_pillar_pair(engine, i, xs[i], gs.gap_size());
        } else {
            for top in [false, true] {
                engine.sprites.get_mut(pillar_label(i, top).as_str()).unwrap().translation.x = xs[i];
            }
        }
    }
}

/// Up/down picks a setting, left/right changes it.
fn handle_settings_input(engine: &Engine, gs: &mut GameState) {
    let kb = &engine.keyboard_state;
    let pressed = |keys: &[KeyCode]| kb.just_pressed_any(keys);
    if pressed(&[KeyCode::ArrowUp, KeyCode::KeyW]) {
        gs.selected = (gs.selected + SETTING_COUNT - 1) % SETTING_COUNT;
    }
    if pressed(&[KeyCode::ArrowDown, KeyCode::KeyS]) {
        gs.selected = (gs.selected + 1) % SETTING_COUNT;
    }
    let level = &mut gs.levels[gs.selected];
    if pressed(&[KeyCode::ArrowLeft, KeyCode::KeyA]) {
        *level = level.saturating_sub(1);
    }
    if pressed(&[KeyCode::ArrowRight, KeyCode::KeyD]) {
        *level = (*level + 1).min(LEVELS - 1);
    }
}

fn player_controls(engine: &Engine) -> Controls {
    let kb = &engine.keyboard_state;
    let mouse = &engine.mouse_state;
    Controls {
        flap: kb.pressed(KeyCode::Space) || mouse.pressed(MouseButton::Left),
        glide: kb.pressed(KeyCode::KeyG) || mouse.pressed(MouseButton::Right),
        glide_target: mouse.location(),
    }
}

fn set_text(engine: &mut Engine, label: &str, value: String) {
    if let Some(text) = engine.texts.get_mut(label) {
        text.value = value;
    }
}

/// Shows or hides the menu / pause panel and fills in its text.
fn update_ui(engine: &mut Engine, gs: &GameState) {
    let showing = gs.mode != Mode::Playing;
    if let Some(panel) = engine.sprites.get_mut("menu_panel") {
        panel.translation = if showing { Vec2::ZERO } else { OFFSCREEN };
    }

    let (title, subtitle, controls) = match gs.mode {
        Mode::Menu => (
            "FlutterWorld",
            "Press Enter to play",
            "Space / left click: fly     G / right click: glide to the mouse\nP: pause     R: back to this menu",
        ),
        Mode::Paused => (
            "Paused",
            "Press P or Enter to resume     R: back to the menu",
            "Space / left click: fly     G / right click: glide to the mouse",
        ),
        Mode::Playing => ("", "", ""),
    };
    set_text(engine, "ui_title", title.to_string());
    set_text(engine, "ui_subtitle", subtitle.to_string());
    set_text(engine, "ui_controls", controls.to_string());
    set_text(
        engine,
        "ui_hint",
        if showing { "Up / Down: choose a setting     Left / Right: change it".to_string() } else { String::new() },
    );
    for row in 0..SETTING_COUNT {
        let label = format!("ui_row_{}", row);
        let selected = gs.selected == row;
        let value = if !showing {
            String::new()
        } else if selected {
            format!(">  {}:  < {} >  <", SETTING_NAMES[row], LEVEL_NAMES[row][gs.levels[row]])
        } else {
            format!("{}:  {}", SETTING_NAMES[row], LEVEL_NAMES[row][gs.levels[row]])
        };
        if let Some(text) = engine.texts.get_mut(label.as_str()) {
            text.value = value;
            text.font_size = if selected { 38.0 } else { 32.0 };
        }
    }
}

fn game_logic(engine: &mut Engine, gs: &mut GameState) {
    // Only collisions involving the bird matter; pillars overlapping the floor are just scenery.
    let mut bird_hit = false;
    for event in engine.collision_events.drain(..) {
        if !event.pair.either_contains("user") {
            continue;
        }
        match event.state {
            CollisionState::Begin => bird_hit = true,
            CollisionState::End => {
                if gs.mode == Mode::Playing {
                    print!("YOU GOT THIS!!!!!!\n");
                }
            }
        }
    }

    let enter = engine.keyboard_state.just_pressed(KeyCode::Enter);
    let pause = engine.keyboard_state.just_pressed(KeyCode::KeyP);
    let reset = engine.keyboard_state.just_pressed(KeyCode::KeyR);

    match gs.mode {
        Mode::Menu => {
            handle_settings_input(engine, gs);
            if enter {
                reset_run(engine, gs);
                gs.mode = Mode::Playing;
            } else {
                scroll_world(engine, gs);
                let pairs = read_pairs(engine);
                let player = engine.sprites.get_mut("user").unwrap();
                let controls = ai_controls(player.translation, gs.prev_flap, &pairs);
                step_bird(&mut player.translation, &mut player.rotation, gs, &controls);
                if bird_hit {
                    // The AI crashed: just start its run over.
                    reset_run(engine, gs);
                }
            }
        }
        Mode::Playing => {
            if pause {
                gs.mode = Mode::Paused;
            } else if reset {
                reset_run(engine, gs);
                gs.mode = Mode::Menu;
            } else {
                scroll_world(engine, gs);
                let controls = player_controls(engine);
                let player = engine.sprites.get_mut("user").unwrap();
                step_bird(&mut player.translation, &mut player.rotation, gs, &controls);
                if bird_hit {
                    println!("Oh no! Flutter died! Try again next time!");
                    reset_run(engine, gs);
                    gs.mode = Mode::Menu;
                }
            }
        }
        Mode::Paused => {
            handle_settings_input(engine, gs);
            if pause || enter {
                gs.mode = Mode::Playing;
            } else if reset {
                reset_run(engine, gs);
                gs.mode = Mode::Menu;
            }
        }
    }

    update_ui(engine, gs);
}

/// Adds the dark panel and the text lines used by the menu and pause screens.
fn add_ui(engine: &mut Engine) {
    let panel = engine.add_sprite("menu_panel", "sprite/flutter/menu_panel.png");
    panel.layer = PANEL;
    panel.translation = OFFSCREEN;
    for (label, y, font_size) in UI_TEXTS {
        let text = engine.add_text(label, "");
        text.font = MENU_FONT.to_string();
        text.font_size = font_size;
        text.translation = Vec2::new(0.0, y);
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

    let background: &mut Sprite = game.add_sprite("background", "sprite/flutter/background.png");
    background.translation = Vec2::new(0.0, 0.0);
    background.scale = 1.7;
    background.layer = BACKGROUND;

    let background2: &mut Sprite = game.add_sprite("background2", "sprite/flutter/background.png");
    background2.translation = Vec2::new(1600.0 * 1.7, 0.0);
    background2.scale = 1.7;
    background2.layer = BACKGROUND;

    let floor: &mut Sprite = game.add_sprite("floor", "sprite/flutter/bg_floor.png");
    floor.translation = Vec2::new(0.0, -385.0);
    floor.scale = 1.7;
    floor.layer = FLOOR;
    floor.collision = true;

    let floor2: &mut Sprite = game.add_sprite("floor2", "sprite/flutter/bg_floor.png");
    floor2.translation = Vec2::new(1600.0 * 1.7, -385.0);
    floor2.scale = 1.7;
    floor2.layer = FLOOR;
    floor2.collision = true;

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
    reset_run(&mut game, &mut game_state);
    update_ui(&mut game, &game_state);
    game.add_logic(game_logic);
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
        let mut pairs = [Pair { x: 0.0, e_b: 0.0, e_t: 0.0 }; NUM_PILLARS];
        for i in 0..NUM_PILLARS {
            xs[i] = FIRST_PILLAR_X + i as f32 * spacing;
            pairs[i] = pair_for(i, xs[i], gap);
        }
        let mut bird = Vec2::new(PLAYER_START_X, 0.0);
        let mut rotation = 0.0;
        for frame in 0..frames {
            let recycled = scroll_pillars(&mut xs, BASE_SCROLL_SPEED * gs.speed_scale(), spacing);
            for i in 0..NUM_PILLARS {
                pairs[i] = if recycled[i] { pair_for(i, xs[i], gap) } else { Pair { x: xs[i], ..pairs[i] } };
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
                    assert_eq!(
                        simulate([f, g, s], 20_000),
                        None,
                        "AI crashed with frequency {}, gap {}, speed {}",
                        LEVEL_NAMES[FREQUENCY][f],
                        LEVEL_NAMES[GAP][g],
                        LEVEL_NAMES[SPEED][s]
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
        let idle = Controls { flap: false, glide: false, glide_target: None };
        let mut crashed_at = None;
        for frame in 0..200 {
            step_bird(&mut bird, &mut rotation, &mut gs, &idle);
            if crashed(bird, &[]) {
                crashed_at = Some(frame);
                break;
            }
        }
        assert!(crashed_at.is_some(), "an idle bird should fall into the floor");
    }

    #[test]
    fn pillars_always_cover_floor_and_ceiling() {
        for level in 0..LEVELS {
            let gap = BASE_GAP * GAP_SCALE[level];
            for i in 0..NUM_PILLARS {
                let pair = pair_for(i, 0.0, gap);
                assert!(pair.e_b - PILLAR_LENGTH <= FLOOR_HIDE_Y, "bottom pillar floats at gap level {level}");
                assert!(pair.e_t + PILLAR_LENGTH >= CEILING_COVER_Y, "top pillar stops short at gap level {level}");
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
    fn turn_toward_takes_the_short_way_round() {
        // From just under +PI to just over -PI is a small turn through PI, not a lap back through 0.
        let turned = turn_toward(3.0, -3.0, 0.2);
        assert!((turned - (3.2 - 2.0 * PI)).abs() < 1e-5, "turned the long way: {turned}");
        assert!((turn_toward(0.0, 1.0, 0.25) - 0.25).abs() < 1e-6);
    }
}
