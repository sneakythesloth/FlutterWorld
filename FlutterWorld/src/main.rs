use rusty_engine::{game, prelude::{bevy::input::keyboard::Key, *}};


#[derive(Resource)]
struct GameState {
    _current_score: u128,
    _high_score: u128,
    _health_left: u128,
    start: bool,
}

const BACKGROUND: f32 = 0.0;
const FLOOR: f32 = 1.0;
const USER: f32 = 2.0;
const PILLAR: f32 = 3.0;
const PILLAR_TOP: f32 = 360.0;
const PILLAR_BOTTOM: f32 = -295.0;
const PILLAR_RECYCLE_X: f32 = -680.0;
const PILLAR_PAIRS: [(&str, &str); 3] = [
    ("pillar_1_bottom", "pillar_1_top"),
    ("pillar_2_bottom", "pillar_2_top"),
    ("pillar_3_bottom", "pillar_3_top"),
];

fn game_logic(engine: &mut Engine, game_state: &mut GameState) {
    let player = engine.sprites.get_mut("user").unwrap();
    for event in engine.collision_events.drain(..) {
    match event.state {
        CollisionState::Begin => {
            println!("Oh no! Flutter died! Try again next time!");
            game_state.start = false;
            player.translation.y = 0.0;
        }

        CollisionState::End => {
        print!("YOU GOT THIS!!!!!!\n");
        }
    }
}
let direct = engine.add_text("directions", "Use Space key to fly\nUse G to glide\nUse F to fall faster\nPress the Enter key\n to restart the game");
    direct.font = "font/BitcountPropSingle-VariableFont_CRSV,ELSH,ELXP,slnt,wght.ttf".to_string();
    direct.translation.x -= 450.0;
    direct.translation.y += 50.0;
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

if engine.keyboard_state.just_pressed(KeyCode::Enter) {
    game_state.start = true;
}

let player = engine.sprites.get_mut("user").unwrap();

if player.translation.y > stand_y &&game_state.start == true{
    player.translation.y -= 10.0; // The bird falls at 10 per frame
} else {
    player.translation.y -= 0.0;
}

if engine.keyboard_state.pressed(KeyCode::Space) &&game_state.start == true {
    player.translation.y += 20.0;
}

if engine.keyboard_state.just_released(KeyCode::Space) &&game_state.start == true {
    player.translation.y += 13.0;
}


if engine.keyboard_state.pressed(KeyCode::KeyG) &&player.translation.y > stand_y &&game_state.start == true {
    player.translation.y += 5.0;
}

if engine.keyboard_state.pressed(KeyCode::KeyF) &&player.translation.y >stand_y &&game_state.start == true {
    player.translation.y -= 15.0;
}

if engine.keyboard_state.pressed(KeyCode::KeyR) {
    game_state.start = false;
    player.translation.y = 0.0;
}
if engine.mouse_state.pressed(MouseButton::Left) &&game_state.start == true {
    player.translation.y += 20.0;
}

if engine.mouse_state.pressed(MouseButton::Right) &&player.translation.y > stand_y &&game_state.start == true {
    player.translation.y += 5.0;
}

}

fn main() {
    let mut game = Game::new();

        game.window_settings(Window {
        title: "FlutterWorld".into(),
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
        };
    game.add_logic(game_logic);
    game.run(game_state); //runs the game in the specified game state
    game.run(GameState {_current_score: 0, _high_score: 0, _health_left: 1, start: false }); //establishes the game state when a player first loads the game
}