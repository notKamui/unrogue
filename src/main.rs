use std::cmp::{self, max, min};

use rand::Rng;
use tcod::{
    colors,
    console::{blit, Offscreen, Root},
    input::{self, Event, Key, KeyCode, Mouse},
    map::{FovAlgorithm, Map as FovMap},
    system::set_fps,
    BackgroundFlag, Color, Console, FontLayout, FontType, TextAlignment,
};

const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 43;

const INVENTORY_WIDTH: i32 = 50;

const PANEL_HEIGHT: i32 = 5;
const PANEL_Y: i32 = SCREEN_HEIGHT - PANEL_HEIGHT;
const BAR_WIDTH: i32 = 20;

const MSG_X: i32 = BAR_WIDTH + 2;
const MSG_WIDTH: i32 = SCREEN_WIDTH - BAR_WIDTH - 2;
const MSG_HEIGHT: i32 = PANEL_HEIGHT - 1;

const LIMIT_FPS: i32 = 20;

const PLAYER: usize = 0;

const COLOR_DARK_WALL: Color = Color { r: 0, g: 0, b: 100 };
const COLOR_LIGHT_WALL: Color = Color {
    r: 130,
    g: 110,
    b: 50,
};
const COLOR_DARK_GROUND: Color = Color {
    r: 50,
    g: 50,
    b: 150,
};
const COLOR_LIGHT_GROUND: Color = Color {
    r: 200,
    g: 180,
    b: 50,
};

const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;
const MAX_ROOM_MONSTERS: i32 = 3;
const MAX_ROOM_ITEMS: i32 = 2;

const FOV_ALGORITHM: FovAlgorithm = FovAlgorithm::Basic;
const FOV_LIGHT_WALLS: bool = true;
const TORCH_RADIUS: i32 = 10;

struct Tcod {
    root: Root,
    console: Offscreen,
    panel: Offscreen,
    fov: FovMap,
    key: Key,
    mouse: Mouse,
}

struct Messages {
    messages: Vec<(String, Color)>,
}
impl Messages {
    pub fn new() -> Self {
        Self { messages: vec![] }
    }

    pub fn add<T: Into<String>>(&mut self, message: T, color: Color) {
        self.messages.push((message.into(), color));
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &(String, Color)> {
        self.messages.iter()
    }
}

#[derive(Debug)]
struct Object {
    x: i32,
    y: i32,
    char: char,
    color: colors::Color,
    name: String,
    blocks: bool,
    alive: bool,
    fighter: Option<Fighter>,
    ai: Option<Ai>,
    item: Option<Item>,
}
impl Object {
    pub fn new(x: i32, y: i32, char: char, name: &str, color: colors::Color, blocks: bool) -> Self {
        Object {
            x,
            y,
            char,
            color,
            name: name.into(),
            blocks,
            alive: false,
            fighter: None,
            ai: None,
            item: None,
        }
    }

    pub fn set_position(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn distance_to(&self, other: &Object) -> f32 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        ((dx.pow(2) + dy.pow(2)) as f32).sqrt()
    }

    pub fn take_damage(&mut self, damage: i32, game: &mut Game) {
        if let Some(fighter) = self.fighter.as_mut() {
            if damage > 0 {
                fighter.hp -= damage;
            }
        }
        if let Some(fighter) = self.fighter {
            if fighter.hp <= 0 {
                self.alive = false;
                fighter.on_death.callback(self, game);
            }
        }
    }

    pub fn attack(&mut self, target: &mut Object, game: &mut Game) {
        let damage = self.fighter.map_or(0, |f| f.power) - target.fighter.map_or(0, |f| f.defense);
        if damage > 0 {
            game.messages.add(
                format!(
                    "{} attacks {} for {} hit points.",
                    self.name, target.name, damage
                ),
                colors::WHITE,
            );
            target.take_damage(damage, game);
        } else {
            game.messages.add(
                format!(
                    "{} attacks {} but it has no effect!",
                    self.name, target.name
                ),
                colors::WHITE,
            );
        }
    }

    pub fn draw(&self, con: &mut dyn Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.char, BackgroundFlag::None);
    }

    pub fn move_by(id: usize, dx: i32, dy: i32, map: &Map, objects: &mut [Object]) {
        let (x, y) = objects[id].position();
        let (new_x, new_y) = (x + dx, y + dy);
        if !is_blocked(new_x, new_y, map, objects) {
            objects[id].set_position(new_x, new_y);
        }
    }

    pub fn move_towards(
        id: usize,
        target_x: i32,
        target_y: i32,
        map: &Map,
        objects: &mut [Object],
    ) {
        let dx = target_x - objects[id].x;
        let dy = target_y - objects[id].y;
        let distance = ((dx.pow(2) + dy.pow(2)) as f32).sqrt();
        let dx = (dx as f32 / distance).round() as i32;
        let dy = (dy as f32 / distance).round() as i32;
        Object::move_by(id, dx, dy, map, objects);
    }
}

#[derive(Clone, Copy, Debug)]
struct Tile {
    blocked: bool,
    explored: bool,
    block_sight: bool,
}
impl Tile {
    pub fn empty() -> Self {
        Tile {
            blocked: false,
            explored: false,
            block_sight: false,
        }
    }

    pub fn wall() -> Self {
        Tile {
            blocked: true,
            explored: false,
            block_sight: true,
        }
    }
}

type Map = Vec<Vec<Tile>>;

#[derive(Clone, Copy, Debug)]
struct Rect {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}
impl Rect {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Rect {
            x1: x,
            y1: y,
            x2: x + width,
            y2: y + height,
        }
    }

    pub fn center(&self) -> (i32, i32) {
        let center_x = (self.x1 + self.x2) / 2;
        let center_y = (self.y1 + self.y2) / 2;
        (center_x, center_y)
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x1 <= other.x2 && self.x2 >= other.x1 && self.y1 <= other.y2 && self.y2 >= other.y1
    }
}

struct Game {
    map: Map,
    messages: Messages,
    inventory: Vec<Object>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Item {
    Heal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerAction {
    TookTurn,
    DidntTakeTurn,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Fighter {
    max_hp: i32,
    hp: i32,
    defense: i32,
    power: i32,
    on_death: DeathCallback,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DeathCallback {
    Player,
    Monster,
}
impl DeathCallback {
    pub fn callback(self, object: &mut Object, game: &mut Game) {
        use DeathCallback::*;
        let callback: fn(&mut Object, &mut Game) = match self {
            Player => DeathCallback::player_death,
            Monster => DeathCallback::monster_death,
        };
        callback(object, game);
    }

    fn player_death(player: &mut Object, game: &mut Game) {
        game.messages.add("You died!", colors::RED);
        player.char = '%';
        player.color = colors::DARK_RED;
    }

    fn monster_death(monster: &mut Object, game: &mut Game) {
        game.messages.add(
            format!(
                "{} is dead! You gain {} experience points.",
                monster.name,
                monster.fighter.unwrap().max_hp
            ),
            colors::ORANGE,
        );
        monster.char = '%';
        monster.color = colors::DARK_RED;
        monster.blocks = false;
        monster.fighter = None;
        monster.ai = None;
        monster.name = format!("remains of {}", monster.name);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Ai {
    Basic,
}

fn mut_two<T>(first_index: usize, second_index: usize, items: &mut [T]) -> (&mut T, &mut T) {
    assert!(first_index != second_index);
    let split_at_index = cmp::max(first_index, second_index);
    let (first_slice, second_slice) = items.split_at_mut(split_at_index);
    if first_index < second_index {
        (&mut first_slice[first_index], &mut second_slice[0])
    } else {
        (&mut second_slice[0], &mut first_slice[second_index])
    }
}

fn is_blocked(x: i32, y: i32, map: &Map, objects: &[Object]) -> bool {
    if map[x as usize][y as usize].blocked {
        return true;
    }
    objects
        .iter()
        .any(|object| object.blocks && object.position() == (x, y))
}

fn create_room(room: Rect, map: &mut Map) {
    for x in (room.x1 + 1)..room.x2 {
        for y in (room.y1 + 1)..room.y2 {
            map[x as usize][y as usize] = Tile::empty();
        }
    }
}

fn create_h_tunnel(x1: i32, x2: i32, y: i32, map: &mut Map) {
    for x in min(x1, x2)..=max(x1, x2) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn create_v_tunnel(y1: i32, y2: i32, x: i32, map: &mut Map) {
    for y in min(y1, y2)..=max(y1, y2) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn make_map(objects: &mut Vec<Object>) -> Map {
    let mut map = vec![vec![Tile::wall(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];
    let mut rooms = vec![];

    for _ in 0..MAX_ROOMS {
        let width = rand::thread_rng().gen_range(ROOM_MIN_SIZE..=ROOM_MAX_SIZE);
        let height = rand::thread_rng().gen_range(ROOM_MIN_SIZE..=ROOM_MAX_SIZE);
        let x = rand::thread_rng().gen_range(0..MAP_WIDTH - width);
        let y = rand::thread_rng().gen_range(0..MAP_HEIGHT - height);

        let new_room = Rect::new(x, y, width, height);
        let failed = rooms
            .iter()
            .any(|other_room| new_room.intersects(other_room));
        if failed {
            continue;
        }

        create_room(new_room, &mut map);
        place_objects(new_room, &map, objects);
        let (new_x, new_y) = new_room.center();
        if rooms.is_empty() {
            objects[PLAYER].set_position(new_x, new_y);
        } else {
            let (prev_x, prev_y) = rooms[rooms.len() - 1].center();
            if rand::random() {
                create_h_tunnel(prev_x, new_x, prev_y, &mut map);
                create_v_tunnel(prev_y, new_y, new_x, &mut map);
            } else {
                create_v_tunnel(prev_y, new_y, prev_x, &mut map);
                create_h_tunnel(prev_x, new_x, new_y, &mut map);
            }
        }

        rooms.push(new_room);
    }

    map
}

fn place_objects(room: Rect, map: &Map, objects: &mut Vec<Object>) {
    let num_monsters = rand::thread_rng().gen_range(0..=MAX_ROOM_MONSTERS);
    for _ in 0..num_monsters {
        let x = rand::thread_rng().gen_range(room.x1 + 1..room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1..room.y2);
        if is_blocked(x, y, &map, objects) {
            continue;
        }
        let mut monster = if rand::random::<f32>() < 0.8 {
            let mut orc = Object::new(x, y, 'o', "Orc", colors::DESATURATED_GREEN, true);
            orc.fighter = Some(Fighter {
                max_hp: 10,
                hp: 10,
                defense: 0,
                power: 3,
                on_death: DeathCallback::Monster,
            });
            orc.ai = Some(Ai::Basic);
            orc
        } else {
            let mut troll = Object::new(x, y, 'T', "Troll", colors::DARKER_GREEN, true);
            troll.fighter = Some(Fighter {
                max_hp: 16,
                hp: 16,
                defense: 1,
                power: 4,
                on_death: DeathCallback::Monster,
            });
            troll.ai = Some(Ai::Basic);
            troll
        };
        monster.alive = true;
        objects.push(monster);
    }

    let num_items = rand::thread_rng().gen_range(0..MAX_ROOM_ITEMS);
    for _ in 0..num_items {
        let x = rand::thread_rng().gen_range(room.x1 + 1..room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1..room.y2);
        if is_blocked(x, y, &map, objects) {
            continue;
        }
        let mut object = Object::new(x, y, '!', "Healing potion", colors::VIOLET, false);
        object.item = Some(Item::Heal);
        objects.push(object);
    }
}

fn pick_item_up(object_id: usize, game: &mut Game, objects: &mut Vec<Object>) {
    if game.inventory.len() >= 26 {
        game.messages.add(
            format!(
                "Your inventory is full, cannot pick up {}.",
                objects[object_id].name
            ),
            colors::RED,
        );
    } else {
        let item = objects.swap_remove(object_id);
        game.messages
            .add(format!("You picked up a {}!", item.name), colors::GREEN);
        game.inventory.push(item);
    }
}

fn player_move_or_attack(dx: i32, dy: i32, game: &mut Game, objects: &mut [Object]) {
    let x = objects[PLAYER].x + dx;
    let y = objects[PLAYER].y + dy;
    let target_id = objects
        .iter()
        .position(|object| object.fighter.is_some() && object.position() == (x, y));
    match target_id {
        Some(target_id) => {
            let (player, target) = mut_two(PLAYER, target_id, objects);
            player.attack(target, game);
        }
        None => {
            Object::move_by(PLAYER, dx, dy, &game.map, objects);
        }
    }
}

fn ai_take_turn(monster_id: usize, tcod: &Tcod, game: &mut Game, objects: &mut [Object]) {
    let (monster_x, monster_y) = objects[monster_id].position();
    if tcod.fov.is_in_fov(monster_x, monster_y) {
        if objects[monster_id].distance_to(&objects[PLAYER]) >= 2.0 {
            Object::move_towards(
                monster_id,
                objects[PLAYER].x,
                objects[PLAYER].y,
                &game.map,
                objects,
            );
        } else if objects[PLAYER].fighter.map_or(false, |f| f.hp > 0) {
            let (monster, player) = mut_two(monster_id, PLAYER, objects);
            monster.attack(player, game);
        }
    }
}

fn render_all(tcod: &mut Tcod, objects: &[Object], game: &mut Game, fov_recompute: bool) {
    if fov_recompute {
        let player = &objects[PLAYER];
        tcod.fov.compute_fov(
            player.x,
            player.y,
            TORCH_RADIUS,
            FOV_LIGHT_WALLS,
            FOV_ALGORITHM,
        );
    }

    let mut to_draw: Vec<_> = objects
        .iter()
        .filter(|o| tcod.fov.is_in_fov(o.x, o.y))
        .collect();
    to_draw.sort_by(|o1, o2| o1.blocks.cmp(&o2.blocks));
    for object in &to_draw {
        if tcod.fov.is_in_fov(object.x, object.y) {
            object.draw(&mut tcod.console);
        }
    }

    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let visible = tcod.fov.is_in_fov(x, y);
            let tile = &mut game.map[x as usize][y as usize];
            let color = match (visible, tile.blocked) {
                (false, true) => COLOR_DARK_WALL,
                (false, false) => COLOR_DARK_GROUND,
                (true, true) => COLOR_LIGHT_WALL,
                (true, false) => COLOR_LIGHT_GROUND,
            };
            if visible {
                tile.explored = true;
            }
            if tile.explored {
                tcod.console
                    .set_char_background(x, y, color, BackgroundFlag::Set);
            }
        }
    }

    blit(
        &tcod.console,
        (0, 0),
        (MAP_WIDTH, MAP_HEIGHT),
        &mut tcod.root,
        (0, 0),
        1.0,
        1.0,
    );

    tcod.panel.set_default_background(colors::BLACK);
    tcod.panel.clear();

    let hp = objects[PLAYER].fighter.map_or(0, |f| f.hp);
    let max_hp = objects[PLAYER].fighter.map_or(0, |f| f.max_hp);
    render_bar(
        &mut tcod.panel,
        1,
        1,
        BAR_WIDTH,
        "HP",
        hp,
        max_hp,
        colors::LIGHT_RED,
        colors::DARKER_RED,
    );

    let mut y = MSG_HEIGHT;
    for &(ref msg, color) in game.messages.iter().rev() {
        let msg_height = tcod.panel.get_height_rect(MSG_X, y, MSG_WIDTH, 0, msg);
        y -= msg_height;
        if y < 0 {
            break;
        }
        tcod.panel.set_default_foreground(color);
        tcod.panel.print_rect(MSG_X, y, MSG_WIDTH, 0, msg);
    }

    tcod.panel.set_default_foreground(colors::LIGHT_GREY);
    tcod.panel.print_ex(
        1,
        3,
        BackgroundFlag::None,
        TextAlignment::Left,
        get_names_under_mouse(tcod.mouse, objects, &tcod.fov),
    );

    blit(
        &tcod.panel,
        (0, 0),
        (SCREEN_WIDTH, PANEL_HEIGHT),
        &mut tcod.root,
        (0, PANEL_Y),
        1.0,
        1.0,
    );
}

fn render_bar(
    panel: &mut Offscreen,
    x: i32,
    y: i32,
    total_width: i32,
    name: &str,
    value: i32,
    maximum: i32,
    bar_color: Color,
    back_color: Color,
) {
    let bar_width = (value as f32 / maximum as f32 * total_width as f32) as i32;

    panel.set_default_background(back_color);
    panel.rect(x, y, total_width, 1, false, BackgroundFlag::Screen);

    panel.set_default_background(bar_color);
    if bar_width > 0 {
        panel.rect(x, y, bar_width, 1, false, BackgroundFlag::Screen);
    }

    panel.set_default_foreground(colors::WHITE);
    panel.print_ex(
        x + total_width / 2,
        y,
        BackgroundFlag::None,
        TextAlignment::Center,
        &format!("{}: {}/{}", name, value, maximum),
    );
}

fn get_names_under_mouse(mouse: Mouse, objects: &[Object], fov_map: &FovMap) -> String {
    objects
        .iter()
        .filter(|object| {
            object.position() == (mouse.cx as i32, mouse.cy as i32)
                && fov_map.is_in_fov(object.x, object.y)
        })
        .map(|object| object.name.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

fn handle_keys(tcod: &mut Tcod, game: &mut Game, objects: &mut Vec<Object>) -> PlayerAction {
    use PlayerAction::*;

    let player_alive = objects[PLAYER].alive;

    match (tcod.key, tcod.key.text(), player_alive) {
        (
            Key {
                code: KeyCode::Enter,
                alt: true,
                ..
            },
            _,
            _,
        ) => {
            let fullscreen = tcod.root.is_fullscreen();
            tcod.root.set_fullscreen(!fullscreen);
            DidntTakeTurn
        }
        (
            Key {
                code: KeyCode::Escape,
                ..
            },
            _,
            _,
        ) => Exit,
        (
            Key {
                code: KeyCode::Up, ..
            },
            _,
            true,
        ) => {
            player_move_or_attack(0, -1, game, objects);
            TookTurn
        }
        (
            Key {
                code: KeyCode::Down,
                ..
            },
            _,
            true,
        ) => {
            player_move_or_attack(0, 1, game, objects);
            TookTurn
        }
        (
            Key {
                code: KeyCode::Left,
                ..
            },
            _,
            true,
        ) => {
            player_move_or_attack(-1, 0, game, objects);
            TookTurn
        }
        (
            Key {
                code: KeyCode::Right,
                ..
            },
            _,
            true,
        ) => {
            player_move_or_attack(1, 0, game, objects);
            TookTurn
        }
        (
            Key {
                code: KeyCode::Text,
                ..
            },
            "g",
            true,
        ) => {
            let item_id = objects.iter().position(|object| {
                object.item.is_some() && object.position() == objects[PLAYER].position()
            });
            if let Some(item_id) = item_id {
                pick_item_up(item_id, game, objects);
            }
            DidntTakeTurn
        }
        (
            Key {
                code: KeyCode::Text,
                ..
            },
            "i",
            true,
        ) => {
            inventory_menu(
                &game.inventory,
                "Press the key next to an item to use it, or any other to cancel.\n",
                &mut tcod.root,
            );
            TookTurn
        }
        _ => DidntTakeTurn,
    }
}

fn menu<T: AsRef<str>>(header: &str, options: &[T], width: i32, root: &mut Root) -> Option<usize> {
    assert!(
        options.len() <= 26,
        "Cannot have a menu with more than 26 options."
    );

    let header_height = root.get_height_rect(0, 0, width, SCREEN_HEIGHT, header);
    let height = options.len() as i32 + header_height;

    let mut window = Offscreen::new(width, height);
    window.set_default_foreground(colors::WHITE);
    window.print_rect_ex(
        0,
        0,
        width,
        height,
        BackgroundFlag::None,
        TextAlignment::Left,
        header,
    );

    for (index, option_text) in options.iter().enumerate() {
        let menu_letter = (b'a' + index as u8) as char;
        let text = format!("({}) {}", menu_letter, option_text.as_ref());
        window.print_ex(
            0,
            header_height + index as i32,
            BackgroundFlag::None,
            TextAlignment::Left,
            text,
        );
    }

    let x = SCREEN_WIDTH / 2 - width / 2;
    let y = SCREEN_HEIGHT / 2 - height / 2;
    blit(&window, (0, 0), (width, height), root, (x, y), 1.0, 0.7);

    root.flush();
    let key = root.wait_for_keypress(true);

    if key.printable.is_alphabetic() {
        let index = key.printable.to_ascii_lowercase() as usize - 'a' as usize;
        if index < options.len() {
            Some(index)
        } else {
            None
        }
    } else {
        None
    }
}

fn inventory_menu(inventory: &[Object], header: &str, root: &mut Root) -> Option<usize> {
    let options = inventory
        .iter()
        .map(|item| item.name.clone())
        .collect::<Vec<_>>();
    menu(header, &options, INVENTORY_WIDTH, root)
}

fn main() {
    set_fps(LIMIT_FPS);

    let mut tcod = Tcod {
        root: Root::initializer()
            .font("arial10x10.png", FontLayout::Tcod)
            .font_type(FontType::Greyscale)
            .size(SCREEN_WIDTH, SCREEN_HEIGHT)
            .title("Unrogue")
            .init(),
        console: Offscreen::new(MAP_WIDTH, MAP_HEIGHT),
        panel: Offscreen::new(SCREEN_WIDTH, PANEL_HEIGHT),
        fov: FovMap::new(MAP_WIDTH, MAP_HEIGHT),
        key: Default::default(),
        mouse: Default::default(),
    };

    let mut player = Object::new(25, 23, '@', "Player", colors::WHITE, true);
    player.alive = true;
    player.fighter = Some(Fighter {
        max_hp: 30,
        hp: 30,
        defense: 2,
        power: 5,
        on_death: DeathCallback::Player,
    });
    let mut objects = vec![player];

    let mut game = Game {
        map: make_map(&mut objects),
        messages: Messages::new(),
        inventory: vec![],
    };
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            tcod.fov.set(
                x,
                y,
                !game.map[x as usize][y as usize].block_sight,
                !game.map[x as usize][y as usize].blocked,
            );
        }
    }

    let mut previous_player_position = (-1, -1);

    game.messages.add(
        "Welcome stranger! Prepare to perish in the Tombs of the Ancient Kings.",
        colors::RED,
    );

    while !tcod.root.window_closed() {
        tcod.console.clear();

        match input::check_for_event(input::MOUSE | input::KEY_PRESS) {
            Some((_, Event::Mouse(m))) => tcod.mouse = m,
            Some((_, Event::Key(k))) => tcod.key = k,
            _ => tcod.key = Default::default(),
        }

        let fov_recompute = previous_player_position != objects[PLAYER].position();
        render_all(&mut tcod, &objects, &mut game, fov_recompute);
        tcod.root.flush();

        previous_player_position = objects[PLAYER].position();
        let action = handle_keys(&mut tcod, &mut game, &mut objects);
        if action == PlayerAction::Exit {
            break;
        }
        if objects[PLAYER].alive && action != PlayerAction::DidntTakeTurn {
            for id in 0..objects.len() {
                if objects[id].ai.is_some() {
                    ai_take_turn(id, &tcod, &mut game, &mut objects);
                }
            }
        }
    }
}
