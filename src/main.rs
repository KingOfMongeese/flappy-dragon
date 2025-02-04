#![windows_subsystem = "windows"]
use bracket_lib::prelude::*;
use rand::seq::SliceRandom;
use rand::thread_rng;
use rodio::{Decoder, OutputStream, Sink};
use std::fs::File;
use std::io::BufReader;
use std::thread;
use tempfile::TempDir;
use zip::read::ZipArchive;

const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;
const FRAME_DURATION: f32 = 30.0;
const ENCOURAGEMENT_LIST: [&str; 5] = [
    "AMAZING!",
    "MARVELOUS!",
    "UNSTOPPABLE!",
    "KEYBOARD WIZARD!",
    "FRANTIC FLYING!",
];
const ENCOURAGEMENT_DELAY_START: i32 = 60;
const DEAD_SCREEN_MESSAGE: [&str; 5] = [
    "OOF WE HEARD THAT IN THE STANDS",
    "YOUR ARE GONNA FEEL THAT FOR AWHILE",
    "MAYBE DONT DO THAT NEXT TIME?",
    "SOMEONE CALL THE CLEAN UP CREW",
    "AH THE SATISFYING SOUND OF \"SPLAT\"",
];

const DRAGON_FRAMES: [u16; 5] = [0, 1, 2, 3, 4];
const DEV_CONSOLE_LAYER: usize = 2;
const DRAGON_CONSOLE_LAYER: usize = 1;

fn play_sound(path: String, volume: f32) {
    thread::spawn(move || {
        let (_stream, stream_handle) = OutputStream::try_default().unwrap();
        let sink = Sink::try_new(&stream_handle).unwrap();
        sink.set_volume(volume);

        let file = File::open(path).unwrap();
        let source = Decoder::new(BufReader::new(file)).unwrap();

        sink.append(source);
        sink.sleep_until_end();
    });
}

fn play_bell(volume_mod: i32) {
    play_sound(
        String::from(r"resources\sounds_files\bell.mp3"),
        0.15 * volume_mod as f32,
    );
}

fn play_flap(volume_mod: i32) {
    play_sound(
        String::from(r"resources\sounds_files\flap.wav"),
        0.3 * volume_mod as f32,
    );
}

fn play_splat(volume_mod: i32) {
    play_sound(
        String::from(r"resources\sounds_files\splat.wav"),
        0.04 * volume_mod as f32,
    );
}

fn play_setting_click(volume_mod: i32) {
    play_sound(
        String::from(r"resources\sounds_files\setting.wav"),
        0.04 * volume_mod as f32,
    );
}

fn play_encouragement(volume_mod: i32) {
    play_sound(
        String::from(r"resources\sounds_files\encouragement.wav"),
        0.015 * volume_mod as f32,
    );
}

struct Settings {
    flap_velocity: f32,
    min_gap_size: i32,
    volume: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            flap_velocity: -2.0,
            min_gap_size: 5,
            volume: 5,
        }
    }
}

struct Player {
    x: i32,
    y: i32,
    velocity: f32,
    current_frame: usize,
}

impl Player {
    fn new(x: i32, y: i32) -> Self {
        Player {
            x,
            y,
            velocity: 0.0,
            current_frame: 0,
        }
    }

    fn render(&mut self, ctx: &mut BTerm) {
        ctx.set_active_console(DRAGON_CONSOLE_LAYER);
        ctx.cls();
        ctx.set_fancy(
            PointF::new(1.0, self.y as f32),
            1,
            Degrees::new(0.0),
            PointF::new(3.0, 3.0),
            WHITE,
            LIGHTBLUE4,
            DRAGON_FRAMES[self.current_frame],
        );
        ctx.set_active_console(0);
    }

    fn gravity_and_move(&mut self) {
        if self.velocity < 2.0 {
            self.velocity += 0.2;
        }

        self.y += self.velocity as i32;
        self.x += 1;
        if self.y < 0 {
            self.y = 0;
        }

        self.current_frame += 1;
        self.current_frame %= 5;
    }

    fn flap(&mut self, velocity: f32, volume_mod: i32) {
        self.velocity = velocity;
        play_flap(volume_mod);
    }
}

struct Obstacle {
    x: i32,
    gap_center_y: i32,
    size: i32,
}

impl Obstacle {
    fn new(x: i32, score: i32, min_gap_size: i32) -> Self {
        let mut random = RandomNumberGenerator::new();
        Obstacle {
            x,
            gap_center_y: random.range(10, 40),
            size: i32::max(min_gap_size, 20 - score),
        }
    }

    fn render(&mut self, ctx: &mut BTerm, player_x: i32) {
        let screen_x = self.x - player_x;
        let half_size = self.size / 2;

        //Draw top half
        for y in 0..self.gap_center_y - half_size {
            ctx.set(screen_x, y, RED, BLACK, to_cp437('|'));
        }

        for y in self.gap_center_y + half_size..SCREEN_HEIGHT {
            ctx.set(screen_x, y, RED, BLACK, to_cp437('|'));
        }
    }

    fn hit_obstacle(&self, player: &Player) -> bool {
        let half_size = self.size / 2;
        let does_x_match = player.x == self.x;
        let player_above_gap = player.y < self.gap_center_y - half_size;
        let player_below_gap = player.y > self.gap_center_y + half_size;
        does_x_match && (player_above_gap || player_below_gap)
    }
}

enum GameMode {
    Menu,
    Playing,
    End,
    Settings,
    Paused,
}

struct State {
    mode: GameMode,
    player: Player,
    frame_time: f32,
    obstacle: Obstacle,
    score: i32,
    settings: Settings,
    dev_toggle: bool,
    encouragement_delay_cnt: i32,
    current_encouragement: String,
    dead_screen_msg: String,
}

impl State {
    fn new() -> Self {
        State {
            mode: GameMode::Menu,
            player: Player::new(5, 25),
            frame_time: 0.0,
            score: 0,
            settings: Settings::default(),
            obstacle: Obstacle::new(SCREEN_WIDTH, 0, 2),
            dev_toggle: false,
            encouragement_delay_cnt: 0,
            current_encouragement: String::from(""),
            dead_screen_msg: String::from(""),
        }
    }

    fn play(&mut self, ctx: &mut BTerm) {
        if let GameMode::Paused = self.mode {
            if let Some(VirtualKeyCode::P) = ctx.key {
                self.mode = GameMode::Playing
            }
            return;
        }

        ctx.cls_bg(LIGHTBLUE4);
        self.frame_time += ctx.frame_time_ms;
        if self.frame_time > FRAME_DURATION {
            self.frame_time = 0.0;
            self.player.gravity_and_move();
        }

        ctx.set_active_console(DEV_CONSOLE_LAYER);
        ctx.cls();
        ctx.set_active_console(0);

        //render ground
        // must be before obstacle
        for x in 0..SCREEN_WIDTH {
            ctx.set(x, SCREEN_HEIGHT - 1, GREEN4, GREEN4, to_cp437('D'));
        }

        if let Some(key) = ctx.key {
            match key {
                VirtualKeyCode::Space => self
                    .player
                    .flap(self.settings.flap_velocity, self.settings.volume),
                VirtualKeyCode::D => self.dev_toggle = !self.dev_toggle,
                VirtualKeyCode::P => {
                    self.mode = GameMode::Paused;
                    ctx.print_centered(5, "(P) Paused");
                }
                _ => (),
            }
        }

        self.player.render(ctx);
        ctx.print(0, 0, "Press Space to flap ><");
        ctx.print(0, 1, format!("Score {}", self.score));

        if self.encouragement_delay_cnt > 0 {
            ctx.print_centered(6, &self.current_encouragement);
            self.encouragement_delay_cnt -= 1;
            if self.encouragement_delay_cnt < 0 {
                self.encouragement_delay_cnt = 0;
            }
        }

        if self.dev_toggle {
            ctx.print_right(
                SCREEN_WIDTH - 1,
                0,
                format!("x,y: {}, {}", self.player.x, self.player.y),
            );
            ctx.print_right(
                SCREEN_WIDTH - 1,
                1,
                format!("flap_velocity: {}", self.settings.flap_velocity),
            );
            ctx.print_right(
                SCREEN_WIDTH - 1,
                2,
                format!("min_gap_size: {}", self.settings.min_gap_size),
            );
            ctx.print_right(
                SCREEN_WIDTH - 1,
                3,
                format!("Current Obstable Gap Size: {}", self.obstacle.size),
            );
            ctx.print_centered(0, "(D) DEV VIEW");
            ctx.set_active_console(DEV_CONSOLE_LAYER);
            ctx.cls();
            ctx.set(1, self.player.y, YELLOW, YELLOW, 3);
            ctx.set_active_console(0);
        }

        self.obstacle.render(ctx, self.player.x);
        if self.player.x > self.obstacle.x {
            play_bell(self.settings.volume);
            self.score += 1;
            if self.score % 5 == 0 {
                let mut rng = thread_rng();
                if let Some(saying) = ENCOURAGEMENT_LIST.choose(&mut rng) {
                    self.current_encouragement = String::from(*saying);
                    self.encouragement_delay_cnt = ENCOURAGEMENT_DELAY_START;
                    play_encouragement(self.settings.volume);
                }
            }
            self.obstacle = Obstacle::new(
                self.player.x + SCREEN_WIDTH,
                self.score,
                self.settings.min_gap_size,
            );
        }

        if self.player.y > SCREEN_HEIGHT || self.obstacle.hit_obstacle(&self.player) {
            ctx.set_active_console(DEV_CONSOLE_LAYER);
            ctx.cls();
            ctx.set_active_console(0);

            ctx.set_active_console(DRAGON_CONSOLE_LAYER);
            ctx.cls();
            ctx.set_active_console(0);

            self.mode = GameMode::End;
            let mut rng = thread_rng();
            play_splat(self.settings.volume);
            ctx.set(1, self.player.y, RED, BLACK, to_cp437('@'));
            ctx.set(0, self.player.y, RED, BLACK, to_cp437('@'));
            ctx.set(1, self.player.y + 1, RED, BLACK, to_cp437('@'));
            ctx.set(1, self.player.y - 1, RED, BLACK, to_cp437('@'));
            ctx.set(2, self.player.y, RED, BLACK, to_cp437('@'));

            if let Some(saying) = DEAD_SCREEN_MESSAGE.choose(&mut rng) {
                self.dead_screen_msg = String::from(*saying);
            }
        }
    }

    fn restart(&mut self) {
        self.player = Player::new(5, 25);
        self.frame_time = 0.0;
        self.obstacle = Obstacle::new(SCREEN_WIDTH, 0, self.settings.min_gap_size);
        self.mode = GameMode::Playing;
        self.score = 0;
        self.encouragement_delay_cnt = 0;
    }

    fn main_menu(&mut self, ctx: &mut BTerm) {
        ctx.cls();
        ctx.print_centered(5, "Your dragon awaits");
        ctx.print_centered(8, "(P) Play");
        ctx.print_centered(9, "(Q) Quit");
        ctx.print_centered(10, "(S) Settings");

        if let Some(key) = ctx.key {
            match key {
                VirtualKeyCode::P => self.restart(),
                VirtualKeyCode::Q => ctx.quitting = true,
                VirtualKeyCode::S => self.mode = GameMode::Settings,
                _ => {}
            }
        }
    }

    fn dead(&mut self, ctx: &mut BTerm) {
        ctx.cls();
        ctx.print_centered(5, "GAME OVER");
        ctx.print_centered(6, format!("Score: {}", self.score));
        ctx.print_centered(8, "(P) Play");
        ctx.print_centered(9, "(Q) Quit");
        ctx.print_centered(10, "(S) Settings");
        ctx.print_centered(15, &self.dead_screen_msg);

        if let Some(key) = ctx.key {
            match key {
                VirtualKeyCode::P => self.restart(),
                VirtualKeyCode::Q => ctx.quitting = true,
                VirtualKeyCode::S => self.mode = GameMode::Settings,
                _ => {}
            }
        }
    }

    fn settings_menu(&mut self, ctx: &mut BTerm) {
        ctx.cls();

        if let Some(key) = ctx.key {
            match key {
                VirtualKeyCode::M => {
                    self.mode = GameMode::Menu;
                    play_setting_click(self.settings.volume);
                }
                VirtualKeyCode::F => {
                    self.settings.flap_velocity -= 0.5;
                    if self.settings.flap_velocity < -4.0 {
                        self.settings.flap_velocity = -1.5;
                    }
                    play_setting_click(self.settings.volume);
                }
                VirtualKeyCode::G => {
                    self.settings.min_gap_size += 1;
                    if self.settings.min_gap_size > 10 {
                        self.settings.min_gap_size = 1;
                    }
                    play_setting_click(self.settings.volume);
                }

                VirtualKeyCode::V => {
                    self.settings.volume += 1;
                    if self.settings.volume > 10 {
                        self.settings.volume = 0;
                    }
                    play_setting_click(self.settings.volume);
                }
                _ => (),
            }
        }

        ctx.print_centered(5, "SETTINGS");
        ctx.print_centered(
            6,
            format!("(F) Flap Velocity: {}", self.settings.flap_velocity),
        );
        ctx.print_centered(
            7,
            format!("(G) Minimum Gap Size: {}", self.settings.min_gap_size),
        );
        ctx.print_centered(8, format!("(V) Volume: {}", self.settings.volume));
        ctx.print_centered(10, "(M) Main Menu");
        ctx.print_right(
            SCREEN_WIDTH - 1,
            SCREEN_HEIGHT - 1,
            "Enter key () to adjust values",
        );
    }
}

impl GameState for State {
    fn tick(&mut self, ctx: &mut BTerm) {
        match self.mode {
            GameMode::Menu => self.main_menu(ctx),
            GameMode::End => self.dead(ctx),
            GameMode::Playing | GameMode::Paused => self.play(ctx),
            GameMode::Settings => self.settings_menu(ctx),
        }
    }
}

fn load_sprites() -> TempDir {
    let temp_dir = TempDir::new().expect("failed to make tempdir");
    let temp_dir_path = temp_dir.path();

    let zip_file_path = r"resources\sprites.zip";
    let file = File::open(zip_file_path).expect("Failed to get file path");
    let mut archive = ZipArchive::new(file).expect("Failed to get archive path");

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).expect("Failed to get index");
        let out_path = temp_dir_path.join(file.name());

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }

        let mut out_file = File::create(out_path).expect("Failed to Extract");
        std::io::copy(&mut file, &mut out_file).expect("Failed to copy");
    }

    temp_dir
}

fn main() -> BError {
    let temp_dir = load_sprites();
    let temp_dir_path = temp_dir.path();
    let term_file_path = format!(r"{}\terminal8x8.png", temp_dir_path.display());
    let dragon_file_path = format!(r"{}\DragonHatchling_Sprites.png", temp_dir_path.display());

    let context = BTermBuilder::new()
        .with_font(&term_file_path, 8, 8)
        .with_font(&dragon_file_path, 32, 32)
        .with_simple_console(SCREEN_WIDTH, SCREEN_HEIGHT, &term_file_path)
        .with_fancy_console(SCREEN_WIDTH, SCREEN_HEIGHT, &dragon_file_path)
        .with_fancy_console(SCREEN_WIDTH, SCREEN_HEIGHT, &term_file_path)
        .with_title("Flappy Dragon")
        .with_tile_dimensions(16, 16)
        .with_fitscreen(true)
        .build()?;
    main_loop(context, State::new())
}
