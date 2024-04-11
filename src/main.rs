use std::cmp::{max, min};

use rand::Rng;
use tcod::{
    colors,
    console::{blit, Offscreen, Root},
    input::{Key, KeyCode},
    system::set_fps,
    BackgroundFlag, Color, Console, FontLayout, FontType,
};

const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 45;

const LIMIT_FPS: i32 = 20;

const COLOR_DARK_WALL: Color = Color { r: 0, g: 0, b: 100 };
const COLOR_DARK_GROUND: Color = Color {
    r: 50,
    g: 50,
    b: 150,
};

const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;

struct Tcod {
    root: Root,
    console: Offscreen,
}

#[derive(Debug)]
struct Object {
    x: i32,
    y: i32,
    char: char,
    color: colors::Color,
}
impl Object {
    pub fn new(x: i32, y: i32, char: char, color: colors::Color) -> Self {
        Object { x, y, char, color }
    }

    pub fn move_by(&mut self, dx: i32, dy: i32, game: &Game) {
        let new_x = self.x + dx;
        let new_y = self.y + dy;
        if game.map[new_x as usize][new_y as usize].blocked {
            return;
        }
        self.x = new_x;
        self.y = new_y;
    }

    pub fn draw(&self, con: &mut dyn Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.char, BackgroundFlag::None);
    }
}

#[derive(Clone, Copy, Debug)]
struct Tile {
    blocked: bool,
    block_sight: bool,
}
impl Tile {
    pub fn empty() -> Self {
        Tile {
            blocked: false,
            block_sight: false,
        }
    }

    pub fn wall() -> Self {
        Tile {
            blocked: true,
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
}

fn make_map(player: &mut Object) -> Map {
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
        let (new_x, new_y) = new_room.center();
        if rooms.is_empty() {
            player.x = new_x;
            player.y = new_y;
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

fn render_all(tcod: &mut Tcod, objects: &[Object], game: &Game) {
    for object in objects {
        object.draw(&mut tcod.console);
    }

    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let wall = game.map[x as usize][y as usize].block_sight;
            if wall {
                tcod.console
                    .set_char_background(x, y, COLOR_DARK_WALL, BackgroundFlag::Set);
            } else {
                tcod.console
                    .set_char_background(x, y, COLOR_DARK_GROUND, BackgroundFlag::Set);
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
}

fn key_to_vector(key: Key) -> (i32, i32) {
    match key {
        Key {
            code: KeyCode::Up, ..
        } => (0, -1),
        Key {
            code: KeyCode::Down,
            ..
        } => (0, 1),
        Key {
            code: KeyCode::Left,
            ..
        } => (-1, 0),
        Key {
            code: KeyCode::Right,
            ..
        } => (1, 0),
        _ => (0, 0),
    }
}

fn handle_keys(tcod: &mut Tcod, game: &Game, player: &mut Object) -> bool {
    let key = tcod.root.wait_for_keypress(true);

    match key {
        Key {
            code: KeyCode::Enter,
            alt: true,
            ..
        } => {
            let fullscreen = tcod.root.is_fullscreen();
            tcod.root.set_fullscreen(!fullscreen);
        }
        Key {
            code: KeyCode::Escape,
            ..
        } => return true,
        _ => {}
    }

    let (dx, dy) = key_to_vector(key);
    player.move_by(dx, dy, game);
    false
}

fn main() {
    set_fps(LIMIT_FPS);

    let root = Root::initializer()
        .font("arial10x10.png", FontLayout::Tcod)
        .font_type(FontType::Greyscale)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Unrogue")
        .init();

    let console = Offscreen::new(MAP_WIDTH, MAP_HEIGHT);

    let mut tcod = Tcod { root, console };

    let player = Object::new(25, 23, '@', colors::WHITE);
    let npc = Object::new(MAP_WIDTH / 2 - 5, MAP_HEIGHT / 2, '@', colors::YELLOW);
    let mut objects = [player, npc];

    let game = Game {
        map: make_map(&mut objects[0]),
    };

    while !tcod.root.window_closed() {
        tcod.console.set_default_foreground(colors::WHITE);
        tcod.console.clear();

        render_all(&mut tcod, &objects, &game);
        tcod.root.flush();

        if handle_keys(&mut tcod, &game, &mut objects[0]) {
            break;
        }
    }
}
