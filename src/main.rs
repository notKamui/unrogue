use std::cmp::{max, min};

use rand::Rng;
use tcod::{
    colors,
    console::{blit, Offscreen, Root},
    input::{Key, KeyCode},
    map::{FovAlgorithm, Map as FovMap},
    system::set_fps,
    BackgroundFlag, Color, Console, FontLayout, FontType,
};

const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 45;

const LIMIT_FPS: i32 = 20;

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

const FOV_ALGORITHM: FovAlgorithm = FovAlgorithm::Basic;
const FOV_LIGHT_WALLS: bool = true;
const TORCH_RADIUS: i32 = 10;

struct Tcod {
    root: Root,
    console: Offscreen,
    fov: FovMap,
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
        place_objects(new_room, objects);
        let (new_x, new_y) = new_room.center();
        if rooms.is_empty() {
            let player = &mut objects[0];
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

fn place_objects(room: Rect, objects: &mut Vec<Object>) {
    let num_monsters = rand::thread_rng().gen_range(0..=MAX_ROOM_MONSTERS);
    for _ in 0..num_monsters {
        let x = rand::thread_rng().gen_range(room.x1 + 1..room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1..room.y2);
        let monster = if rand::random::<f32>() < 0.8 {
            Object::new(x, y, 'o', colors::DESATURATED_GREEN)
        } else {
            Object::new(x, y, 'T', colors::DARKER_GREEN)
        };
        objects.push(monster);
    }
}

fn render_all(tcod: &mut Tcod, objects: &[Object], game: &mut Game, fov_recompute: bool) {
    if fov_recompute {
        let player = &objects[0];
        tcod.fov.compute_fov(
            player.x,
            player.y,
            TORCH_RADIUS,
            FOV_LIGHT_WALLS,
            FOV_ALGORITHM,
        );
    }

    for object in objects {
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

    let mut tcod = Tcod {
        root: Root::initializer()
            .font("arial10x10.png", FontLayout::Tcod)
            .font_type(FontType::Greyscale)
            .size(SCREEN_WIDTH, SCREEN_HEIGHT)
            .title("Unrogue")
            .init(),
        console: Offscreen::new(MAP_WIDTH, MAP_HEIGHT),
        fov: FovMap::new(MAP_WIDTH, MAP_HEIGHT),
    };

    let player = Object::new(25, 23, '@', colors::WHITE);
    let mut objects = vec![player];

    let mut game = Game {
        map: make_map(&mut objects),
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

    while !tcod.root.window_closed() {
        tcod.console.set_default_foreground(colors::WHITE);
        tcod.console.clear();

        let fov_recompute = previous_player_position != (objects[0].x, objects[0].y);
        render_all(&mut tcod, &objects, &mut game, fov_recompute);
        tcod.root.flush();

        previous_player_position = (objects[0].x, objects[0].y);
        if handle_keys(&mut tcod, &game, &mut objects[0]) {
            break;
        }
    }
}
