use rusty_engine::{game, prelude::{bevy::input::keyboard::Key, *}};
use std::f32::consts::{FRAC_PI_2, PI};

/// Rotates `current` toward `target` by at most `max_step` radians, taking the shorter way
/// round, and keeps the result in (-PI, PI].
fn turn_toward(current: f32, target: f32, max_step: f32) -> f32 {
    let diff = (target - current + PI).rem_euclid(2.0 * PI) - PI;
    let turned = current + diff.clamp(-max_step, max_step);
    (turned + PI).rem_euclid(2.0 * PI) - PI
}


#[derive(Resource)]
struct GameState {
    _current_score: u128,
    _high_score: u128,
    _health_left: u128,
    start: bool,
    lift: f32,
    glide_speed: f32,
    stalled: bool,
    heading: f32, // flight direction in radians: 0 is forward, positive is up
}

const LIFT_ON_RELEASE: f32 = 18.0;
const LIFT_DECAY: f32 = 0.8;
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

const BACKGROUND: f32 = 0.0;
const FLOOR: f32 = 1.0;
const USER: f32 = 2.0;
const PILLAR: f32 = 3.0;
const PILLAR_TOP: f32 = 554.0;
const PILLAR_BOTTOM: f32 = -310.0;
const PILLAR_RECYCLE_X: f32 = -(WINDOW_WIDTH as f32) / 2.0 - 120.0; // just past the left edge
const PILLAR_PAIRS: [(&str, &str); 3] = [
    ("pillar_1_bottom", "pillar_1_top"),
    ("pillar_2_bottom", "pillar_2_top",),
    ("pillar_3_bottom", "pillar_3_top"),
];

fn game_logic(engine: &mut Engine, game_state: &mut GameState) {
    let player = engine.sprites.get_mut("user").unwrap();
    for event in engine.collision_events.drain(..) {
    match event.state {
        CollisionState::Begin => {
            println!("Oh no! Flutter died! Try again next time!");
            game_state.start = false;
            game_state.lift = 0.0;
            game_state.glide_speed = GLIDE_BASE_SPEED;
            game_state.stalled = false;
            game_state.heading = 0.0;
            player.translation = Vec2::new(PLAYER_START_X, 0.0);
            player.rotation = 0.0;
        }

        CollisionState::End => {
        print!("YOU GOT THIS!!!!!!\n");
        }
    }
}

let direct = engine.add_text("directions", "Use Space key to fly\nHold G or right click to glide to the mouse\nPress the Enter key\n to restart the game");
direct.font = "font/BitcountPropSingle-VariableFont_CRSV,ELSH,ELXP,slnt,wght.ttf".to_string();
direct.translation.x -= 450.0;
direct.translation.y += 150.0;

    if game_state.start == true {
        direct.value = format!("");
    }

let stand_y = -295.0;

if game_state.start {
    for floor_label in ["floor", "floor2"] {
        engine.sprites.get_mut(floor_label).unwrap().translation.x -= 6.0;
            for bg_label in ["background", "background2"] {
        engine.sprites.get_mut(bg_label).unwrap().translation.x -= 1.5;
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
    }

    for (bottom_label, top_label) in PILLAR_PAIRS {
        let next_x = engine.sprites.get(bottom_label).unwrap().translation.x - 6.0;
        if next_x < PILLAR_RECYCLE_X {
            let rightmost_x = PILLAR_PAIRS
                .iter()
                .map(|(bottom_label, _)| engine.sprites.get(*bottom_label).unwrap().translation.x)
                .fold(f32::NEG_INFINITY, f32::max);
            let recycled_x = rightmost_x + 650.0;
            for pillar_label in [bottom_label, top_label] {
                engine.sprites.get_mut(pillar_label).unwrap().translation.x = recycled_x;
            }
        } else {
            for pillar_label in [bottom_label, top_label] {
                engine.sprites.get_mut(pillar_label).unwrap().translation.x = next_x;
            }
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
}

let player = engine.sprites.get_mut("user").unwrap();

if engine.keyboard_state.just_pressed(KeyCode::Enter) {
    game_state.start = true;
}

let glide = engine.keyboard_state.pressed(KeyCode::KeyG) || engine.mouse_state.pressed(MouseButton::Right);

// Gravity is suspended while gliding; the glide's own speed model takes over (unless stalled).
if player.translation.y > stand_y &&game_state.start == true &&(!glide || game_state.stalled) {
    player.translation.y -= 10.0; // The bird falls at 10 per frame
}

if engine.keyboard_state.pressed(KeyCode::Space) &&game_state.start == true {
    player.translation.y += 20.0;
}

// Momentum: releasing the flap key starts a small upward carry-over that fades out,
// so the bird eases into falling instead of stopping dead.
if engine.keyboard_state.just_released(KeyCode::Space) &&game_state.start == true {
    game_state.lift = LIFT_ON_RELEASE;
}

if game_state.start && game_state.lift > 0.0 {
    player.translation.y += game_state.lift;
    game_state.lift *= LIFT_DECAY;
    if game_state.lift < 1.0 {
        game_state.lift = 0.0;
    }
}


// A stalled bird has lost too much speed climbing: it can't glide, so it falls (gravity
// above) and the fall rebuilds speed until it can steer again.
if game_state.start && game_state.stalled {
    game_state.glide_speed += STALL_DIVE_ACCEL;
    game_state.heading = turn_toward(game_state.heading, -FRAC_PI_2, STALL_NOSE_DROP);
    if game_state.glide_speed >= GLIDE_RECOVER_SPEED {
        game_state.stalled = false;
    }
}

// Gliding: head toward the mouse, trading height for speed. Heading downhill builds
// speed, climbing bleeds it off, and letting go of the glide key drains it back to base.
let mut target_tilt = if game_state.stalled { STALL_TILT } else { 0.0 };
if game_state.start && !game_state.stalled {
    if glide {
        // The bird turns toward the mouse at a rate set by its speed, so a slow bird barely
        // turns at all. Speed gain and loss follow where the bird is actually pointing.
        if let Some(mouse) = engine.mouse_state.location() {
            let to_mouse = mouse - player.translation;
            if to_mouse.length() > GLIDE_ARRIVE_DIST {
                let desired = to_mouse.y.atan2(to_mouse.x);
                let turn_rate = TURN_RATE_PER_SPEED * game_state.glide_speed;
                game_state.heading = turn_toward(game_state.heading, desired, turn_rate);
            }
        }
        let (sin, cos) = game_state.heading.sin_cos();
        if sin < 0.0 {
            game_state.glide_speed += GLIDE_DIVE_ACCEL * -sin;
        } else {
            game_state.glide_speed -= GLIDE_CLIMB_DRAG * sin;
        }
        game_state.glide_speed = game_state.glide_speed.clamp(GLIDE_MIN_SPEED, GLIDE_MAX_SPEED);
        if sin > 0.0 && game_state.glide_speed <= GLIDE_STALL_SPEED {
            game_state.stalled = true;
            target_tilt = STALL_TILT;
        } else {
            // Use |cos| so the nose never flips backwards: flying back is only tilted by
            // how far up or down the bird is heading.
            target_tilt = sin.atan2(cos.abs()).clamp(-MAX_TILT, MAX_TILT);
            player.translation += Vec2::new(cos, sin) * game_state.glide_speed;
        }
    } else {
        game_state.glide_speed = (game_state.glide_speed - GLIDE_COAST_DRAG).max(GLIDE_BASE_SPEED);
        game_state.heading = turn_toward(game_state.heading, 0.0, TURN_RATE_PER_SPEED * GLIDE_BASE_SPEED);
    }
}
player.rotation += (target_tilt - player.rotation) * TILT_SMOOTHING;

// A small headwind pushes the bird backwards, so it has to glide forward to hold its place.
if game_state.start {
    player.translation.x -= WIND_SPEED;
}

// Keep the bird inside the window on every side.
player.translation.x = player.translation.x.clamp(PLAYER_MIN_X, PLAYER_MAX_X);
player.translation.y = player.translation.y.clamp(PLAYER_MIN_Y, PLAYER_MAX_Y);

if engine.keyboard_state.pressed(KeyCode::KeyR) {
    game_state.start = false;
    game_state.lift = 0.0;
    game_state.glide_speed = GLIDE_BASE_SPEED;
    game_state.stalled = false;
    game_state.heading = 0.0;
    player.translation = Vec2::new(PLAYER_START_X, 0.0);
    player.rotation = 0.0;
}
if engine.mouse_state.pressed(MouseButton::Left) &&game_state.start == true {
    player.translation.y += 20.0;
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
    player.translation = Vec2::new(0.0, 0.0);
    player.scale = 0.1;
    player.translation.x -= 450.0;
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

    for (bottom_label, top_label, bottom_path, bottom_image_height, top_path, top_image_height, x) in [
        ("pillar_1_bottom", "pillar_1_top", "sprite/flutter/short_pillar.png", 448.0, "sprite/flutter/long_pillar.png", 1120.0, 4000.0),
        ("pillar_2_bottom", "pillar_2_top", "sprite/flutter/average_pillar.png", 704.0, "sprite/flutter/average_pillar.png", 704.0, 4000.0),
        ("pillar_3_bottom", "pillar_3_top", "sprite/flutter/long_pillar.png", 1120.0, "sprite/flutter/short_pillar.png", 348.0, 4000.0),
        ("pillar_4_bottom", "pillar_4_top", "sprite/flutter/smallest_pillar.png", 1120.0, "sprite/flutter/smallest_pillar.png", 348.0, 4000.0)
    ] {
        let bottom_height = bottom_image_height * 0.3;
        let top_height = top_image_height * 0.3;
        let bottom_b = top_image_height * 0.1;

        let bottom = game.add_sprite(bottom_label, bottom_path);
        bottom.scale = 0.3;
        bottom.translation = Vec2::new(x, PILLAR_BOTTOM + bottom_height * 0.5);
        bottom.layer = PILLAR;
        bottom.collision = true;

        let top = game.add_sprite(top_label, top_path);
        top.scale = 0.3;
        top.translation = Vec2::new(x, PILLAR_TOP - top_height * 0.5);
        top.rotation = std::f32::consts::PI;
        top.layer = PILLAR;
        top.collision = true;
    }

    let game_state = GameState {
        _current_score: 0,
        _high_score: 0,
        _health_left: 1,
        start: false,
        lift: 0.0,
        glide_speed: GLIDE_BASE_SPEED,
        stalled: false,
        heading: 0.0,
        };
    game.add_logic(game_logic);
    game.run(game_state); //runs the game in the specified game state
    game.run(GameState {_current_score: 0, _high_score: 0, _health_left: 1, start: false, lift: 0.0, glide_speed: GLIDE_BASE_SPEED, stalled: false, heading: 0.0 }); //establishes the game state when a player first loads the game
}
