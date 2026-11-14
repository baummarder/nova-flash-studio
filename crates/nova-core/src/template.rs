//! Built-in project templates.
//!
//! Templates are real projects: they contain scenes, layers, keyframes, vector
//! text and scripts that go through the normal validator/compiler/exporter.
//! They are intentionally small so they remain useful starting points without
//! hiding unfinished engine features.

use crate::model::{
    Easing, Frame, Layer, ObjectInstance, ObjectKind, Project, Rgba, Scene, Script, Symbol,
    TextAlign, TextFont, Tween,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectTemplate {
    Empty,
    AnimationShort,
    Platformer,
    UiKit,
}

impl ProjectTemplate {
    pub const ALL: [Self; 4] = [
        Self::Empty,
        Self::AnimationShort,
        Self::Platformer,
        Self::UiKit,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::AnimationShort => "animation",
            Self::Platformer => "platformer",
            Self::UiKit => "ui",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Empty => "Leeres Projekt",
            Self::AnimationShort => "Animations-Kurzfilm",
            Self::Platformer => "Plattformspiel",
            Self::UiKit => "UI-Kit",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Empty => "Eine Szene, eine Ebene und ein leeres Main-Skript.",
            Self::AnimationShort => {
                "Zwei Szenen, Vektortext, Symbole und Motion-Tweens; alle Szenen werden exportiert."
            }
            Self::Platformer => {
                "Spielbare Figur mit Tastatursteuerung, Plattformen, Münzen und HUD-Text."
            }
            Self::UiKit => "Panel-, Karten- und Button-Layout mit Klick-Skript.",
        }
    }

    pub fn create(self, name: impl Into<String>) -> Project {
        match self {
            Self::Empty => Project::new_empty(name),
            Self::AnimationShort => animation_short(name),
            Self::Platformer => platformer(name),
            Self::UiKit => ui_kit(name),
        }
    }
}

fn animation_short(name: impl Into<String>) -> Project {
    let mut project = Project::new_empty(name);
    project.metadata.description = ProjectTemplate::AnimationShort.description().into();
    project.export.export_all_scenes = true;

    let symbol = Symbol::new("Stern", 96.0, 96.0, Rgba::rgb(255, 209, 84));
    let symbol_id = symbol.id;

    let mut intro = Scene::new("Intro", 960.0, 540.0);
    intro.background = Rgba::rgb(12, 16, 28);
    intro.fps = 30.0;

    let mut backdrop = Layer::new("Hintergrund", 60);
    backdrop.frames[0] = keyframe(vec![ObjectInstance::rect(
        "Bühne",
        480.0,
        270.0,
        960.0,
        540.0,
        Rgba::rgb(20, 28, 46),
    )]);
    let mut glow = Layer::new("Glow", 60);
    glow.frames[0] = keyframe(vec![ObjectInstance::ellipse(
        "Glow-Punkt",
        760.0,
        130.0,
        280.0,
        280.0,
        Rgba::rgba(61, 214, 196, 70),
    )]);
    let mut title = Layer::new("Titel", 60);
    title.frames[0] = keyframe(vec![text_object(
        "Titel",
        480.0,
        210.0,
        "NØVA",
        128.0,
        Rgba::rgb(245, 166, 35),
        TextAlign::Center,
        true,
        false,
    )]);
    title.frames[0].tween = Some(Tween {
        easing: Easing::EaseOut,
    });
    title.frames[24] = keyframe(vec![text_object(
        "Titel",
        480.0,
        240.0,
        "NØVA",
        128.0,
        Rgba::rgb(255, 209, 84),
        TextAlign::Center,
        true,
        false,
    )]);

    let mut subtitle = Layer::new("Untertitel", 60);
    subtitle.frames[0] = keyframe(vec![text_object(
        "Untertitel",
        480.0,
        320.0,
        "Flash Studio",
        44.0,
        Rgba::rgb(196, 210, 232),
        TextAlign::Center,
        false,
        false,
    )]);

    let mut star = Layer::new("Stern", 60);
    let star_object = ObjectInstance::symbol("Stern", 480.0, 420.0, symbol_id, Rgba::WHITE);
    star.frames[0] = keyframe(vec![star_object.clone()]);
    star.frames[0].tween = Some(Tween {
        easing: Easing::EaseInOut,
    });
    let mut star_end = star_object;
    star_end.transform.rotation = 180.0;
    star_end.transform.scale_x = 0.55;
    star_end.transform.scale_y = 0.55;
    star_end.transform.x = 700.0;
    star_end.transform.y = 80.0;
    star.frames[59] = keyframe(vec![star_end]);

    intro.layers = vec![backdrop, glow, title, subtitle, star];

    let mut end = Scene::new("Outro", 960.0, 540.0);
    end.background = Rgba::rgb(12, 16, 28);
    end.fps = 30.0;
    let mut end_backdrop = Layer::new("Hintergrund", 45);
    end_backdrop.frames[0] = keyframe(vec![ObjectInstance::rect(
        "Bühne",
        480.0,
        270.0,
        960.0,
        540.0,
        Rgba::rgb(20, 28, 46),
    )]);
    let mut end_title = Layer::new("Titel", 45);
    end_title.frames[0] = keyframe(vec![text_object(
        "Titel",
        480.0,
        270.0,
        "ENDE",
        96.0,
        Rgba::rgb(61, 214, 196),
        TextAlign::Center,
        true,
        false,
    )]);
    end_title.frames[0].tween = Some(Tween::default());
    end_title.frames[30] = keyframe(vec![text_object(
        "Titel",
        480.0,
        270.0,
        "ENDE",
        96.0,
        Rgba::rgb(255, 209, 84),
        TextAlign::Center,
        true,
        false,
    )]);
    end.layers = vec![end_backdrop, end_title];

    project.scenes = vec![intro, end];
    project.active_scene = project.scenes.first().map(|scene| scene.id);
    project.symbols = vec![symbol];
    project.scripts.clear();
    project
}

fn platformer(name: impl Into<String>) -> Project {
    let mut project = Project::new_empty(name);
    project.metadata.description = ProjectTemplate::Platformer.description().into();
    project.export.export_all_scenes = true;

    let mut scene = Scene::new("Level 1", 960.0, 540.0);
    scene.background = Rgba::rgb(29, 45, 64);
    scene.fps = 30.0;

    let mut sky = Layer::new("Himmel", 120);
    sky.frames[0] = keyframe(vec![
        ObjectInstance::rect("Himmel", 480.0, 270.0, 960.0, 540.0, Rgba::rgb(23, 37, 55)),
        ObjectInstance::ellipse("Mond", 760.0, 90.0, 110.0, 110.0, Rgba::rgb(245, 232, 189)),
    ]);

    let mut ground = Layer::new("Boden", 120);
    ground.frames[0] = keyframe(vec![
        ObjectInstance::rect("Boden", 480.0, 510.0, 960.0, 60.0, Rgba::rgb(72, 116, 92)),
        ObjectInstance::rect(
            "Plattform links",
            300.0,
            380.0,
            240.0,
            26.0,
            Rgba::rgb(114, 82, 146),
        ),
        ObjectInstance::rect(
            "Plattform rechts",
            680.0,
            300.0,
            210.0,
            26.0,
            Rgba::rgb(114, 82, 146),
        ),
    ]);

    let mut coins = Layer::new("Münzen", 120);
    coins.frames[0] = keyframe(vec![
        ObjectInstance::ellipse("Münze 1", 300.0, 330.0, 26.0, 26.0, Rgba::rgb(255, 209, 84)),
        ObjectInstance::ellipse("Münze 2", 680.0, 250.0, 26.0, 26.0, Rgba::rgb(255, 209, 84)),
        ObjectInstance::ellipse("Münze 3", 760.0, 250.0, 26.0, 26.0, Rgba::rgb(255, 209, 84)),
    ]);

    let mut player_layer = Layer::new("Spieler", 120);
    let player = ObjectInstance::rect("Player", 120.0, 430.0, 46.0, 64.0, Rgba::rgb(61, 214, 196));
    let player_id = player.id;
    player_layer.frames[0] = keyframe(vec![player]);

    let mut hud = Layer::new("HUD", 120);
    hud.frames[0] = keyframe(vec![text_object(
        "HUD",
        24.0,
        34.0,
        "Pfeiltasten: bewegen · Ziel: 3 Münzen",
        22.0,
        Rgba::rgb(210, 223, 240),
        TextAlign::Left,
        false,
        false,
    )]);

    scene.layers = vec![sky, ground, coins, player_layer, hud];
    project.scenes = vec![scene];
    project.active_scene = project.scenes.first().map(|scene| scene.id);

    let mut script = Script::new(
        "scripts/Player.as",
        r#"package {
    import flash.display.MovieClip;
    import flash.events.Event;
    import flash.events.KeyboardEvent;
    import flash.ui.Keyboard;

    public class Player extends MovieClip {
        private var speed:Number = 7;
        private var leftDown:Boolean = false;
        private var rightDown:Boolean = false;
        private var upDown:Boolean = false;
        private var velocityY:Number = 0;

        public function Player() {
            addEventListener(Event.ENTER_FRAME, update);
            stage.addEventListener(KeyboardEvent.KEY_DOWN, onKeyDown);
            stage.addEventListener(KeyboardEvent.KEY_UP, onKeyUp);
        }

        private function onKeyDown(event:KeyboardEvent):void {
            if (event.keyCode == Keyboard.LEFT) { leftDown = true; }
            if (event.keyCode == Keyboard.RIGHT) { rightDown = true; }
            if (event.keyCode == Keyboard.SPACE) { upDown = true; }
        }

        private function onKeyUp(event:KeyboardEvent):void {
            if (event.keyCode == Keyboard.LEFT) { leftDown = false; }
            if (event.keyCode == Keyboard.RIGHT) { rightDown = false; }
            if (event.keyCode == Keyboard.SPACE) { upDown = false; }
        }

        private function update(event:Event):void {
            if (leftDown) { x -= speed; }
            if (rightDown) { x += speed; }
            if (x < 30) { x = 30; }
            if (x > 930) { x = 930; }
            if (upDown) { velocityY = -12; upDown = false; }
            velocityY += 0.9;
            y += velocityY;
            if (y > 470) { y = 470; velocityY = 0; }
        }
    }
}
"#,
    );
    script.linked_object = Some(player_id);
    project.scripts = vec![script];
    project
}

fn ui_kit(name: impl Into<String>) -> Project {
    let mut project = Project::new_empty(name);
    project.metadata.description = ProjectTemplate::UiKit.description().into();
    project.export.export_all_scenes = true;

    let mut scene = Scene::new("UI", 900.0, 600.0);
    scene.background = Rgba::rgb(17, 21, 30);
    scene.fps = 24.0;

    let mut background = Layer::new("Hintergrund", 30);
    background.frames[0] = keyframe(vec![ObjectInstance::rect(
        "Hintergrund",
        450.0,
        300.0,
        900.0,
        600.0,
        Rgba::rgb(17, 21, 30),
    )]);

    let mut panel = Layer::new("Panel", 30);
    panel.frames[0] = keyframe(vec![
        ObjectInstance::rect("Panel", 450.0, 300.0, 680.0, 420.0, Rgba::rgb(28, 35, 50)),
        ObjectInstance::rect(
            "Akzentlinie",
            450.0,
            112.0,
            680.0,
            8.0,
            Rgba::rgb(61, 214, 196),
        ),
    ]);

    let mut labels = Layer::new("Labels", 30);
    labels.frames[0] = keyframe(vec![
        text_object(
            "Überschrift",
            450.0,
            170.0,
            "KONTEN",
            56.0,
            Rgba::rgb(245, 247, 252),
            TextAlign::Center,
            true,
            false,
        ),
        text_object(
            "Beschreibung",
            450.0,
            225.0,
            "Wähle ein Profil oder erstelle ein neues.",
            24.0,
            Rgba::rgb(160, 176, 199),
            TextAlign::Center,
            false,
            false,
        ),
        text_object(
            "Hinweis",
            450.0,
            440.0,
            "Klick auf den Button wird im Trace-Log ausgegeben.",
            20.0,
            Rgba::rgb(110, 126, 150),
            TextAlign::Center,
            false,
            true,
        ),
    ]);

    let mut button = Layer::new("Button", 30);
    let button_object = ObjectInstance::rect(
        "PrimaryButton",
        450.0,
        355.0,
        280.0,
        64.0,
        Rgba::rgb(61, 214, 196),
    );
    let button_id = button_object.id;
    button.frames[0] = keyframe(vec![button_object]);
    let mut button_label = Layer::new("Button-Label", 30);
    button_label.frames[0] = keyframe(vec![text_object(
        "Button-Label",
        450.0,
        355.0,
        "WEITER",
        28.0,
        Rgba::rgb(16, 24, 32),
        TextAlign::Center,
        true,
        false,
    )]);

    scene.layers = vec![background, panel, labels, button, button_label];
    project.scenes = vec![scene];
    project.active_scene = project.scenes.first().map(|scene| scene.id);

    let mut script = Script::new(
        "scripts/PrimaryButton.as",
        r#"package {
    import flash.display.MovieClip;
    import flash.events.MouseEvent;

    public class PrimaryButton extends MovieClip {
        public function PrimaryButton() {
            addEventListener(MouseEvent.CLICK, onClick);
        }

        private function onClick(event:MouseEvent):void {
            scaleX = 0.96;
            scaleY = 0.96;
            trace("PrimaryButton geklickt");
        }
    }
}
"#,
    );
    script.linked_object = Some(button_id);
    project.scripts = vec![script];
    project
}

fn keyframe(objects: Vec<ObjectInstance>) -> Frame {
    Frame {
        keyframe: true,
        objects,
        tween: None,
        label: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn text_object(
    name: &str,
    x: f32,
    y: f32,
    text: &str,
    size: f32,
    fill: Rgba,
    align: TextAlign,
    bold: bool,
    italic: bool,
) -> ObjectInstance {
    let mut object = ObjectInstance::text(name, x, y, text, size, fill);
    if let ObjectKind::Text(value) = &mut object.kind {
        value.align = align;
        value.font = TextFont::NoveSans;
        value.bold = bold;
        value.italic = italic;
    }
    object
}

/// Returns the template used by a stable CLI/GUI identifier.
pub fn template_by_id(id: &str) -> Option<ProjectTemplate> {
    ProjectTemplate::ALL
        .into_iter()
        .find(|template| template.id() == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_is_valid() {
        for template in ProjectTemplate::ALL {
            let project = template.create(format!("Test {}", template.id()));
            project
                .validate()
                .unwrap_or_else(|error| panic!("{}: {error}", template.id()));
            assert!(!project.scenes.is_empty());
        }
    }

    #[test]
    fn templates_are_serializable() {
        for template in ProjectTemplate::ALL {
            let project = template.create("Serialisierung");
            let json = project.to_json().unwrap();
            Project::from_json(&json).unwrap();
        }
    }

    #[test]
    fn ids_are_unique_and_lookup_works() {
        let mut ids = std::collections::HashSet::new();
        for template in ProjectTemplate::ALL {
            assert!(ids.insert(template.id()));
            assert_eq!(template_by_id(template.id()), Some(template));
        }
    }

    #[test]
    fn platformer_links_its_player_script() {
        let project = ProjectTemplate::Platformer.create("Game");
        let script = project.scripts.first().expect("script");
        let linked = script.linked_object.expect("linked object");
        assert!(project.find_object(linked).is_some());
    }
}
