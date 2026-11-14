package {
    import flash.display.MovieClip;
    import flash.display.Graphics;
    import flash.events.Event;
    import flash.events.KeyboardEvent;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import flash.ui.Keyboard;
    import flash.utils.getTimer;
    import game.Input;

    /// Interactive menu: pick a mode with UP/DOWN + SPACE (or ENTER).
    /// Selection starts a placeholder "game" scene inside the same SWF.
    public dynamic class Main extends MovieClip {
        private var items = [];
        private var selected:int = 0;
        private var mode:int = 0; // 0 = menu, 1 = game, 2 = game over
        private var score:int = 0;
        private var lastTime:int = 0;
        private var title:TextField;
        private var hint:TextField;
        private var gameBox:MovieClip;
        private var player:MovieClip;

        public function Main() {
            Input.init(stage);
            buildMenu();
            addEventListener(Event.ENTER_FRAME, update);
        }

        private function label(text:String, size:Number, color:Number, y:Number):TextField {
            var tf:TextField = new TextField();
            var fmt:TextFormat = new TextFormat();
            fmt.size = size;
            fmt.color = color;
            tf.defaultTextFormat = fmt;
            tf.x = stage.stageWidth / 2 - 120;
            tf.y = y;
            tf.text = text;
            addChild(tf);
            return tf;
        }

        private function buildMenu():void {
            mode = 0;
            title = label("NOVA MINI GAMES", 34, 0xFFFFFF, 80);
            var names = ["Pong", "Breakout", "Platformer", "TopDown", "Racing"];
            var i:int;
            for (i = 0; i < names.length; i++) {
                var tf:TextField = label(names[i], 24, 0xCCCCCC, 180 + i * 44);
                items.push(tf);
            }
            hint = label("Pfeile + Leertaste waehlen", 18, 0x888888, 460);
            selected = 0;
        }

        private function clearStage():void {
            var i:int;
            for (i = this.numChildren - 1; i >= 0; i--) {
                removeChild(this.getChildAt(i));
            }
            items = [];
        }

        private function startGame():void {
            clearStage();
            mode = 1;
            score = 0;
            gameBox = new MovieClip();
            var g:Graphics = gameBox.graphics;
            g.beginFill(0x14141E);
            g.drawRect(0, 0, stage.stageWidth, stage.stageHeight);
            g.endFill();
            addChild(gameBox);
            player = new MovieClip();
            var pg:Graphics = player.graphics;
            pg.beginFill(0xFFD24A);
            pg.drawRect(-15, -15, 30, 30);
            pg.endFill();
            player.wx = stage.stageWidth / 2;
            player.wy = stage.stageHeight / 2;
            player.x = player.wx;
            player.y = player.wy;
            gameBox.addChild(player);
            label("Demo-Modus " + (selected + 1) + " - ESC fuer Menue", 20, 0xFFFFFF, 40);
        }

        private function gameOver():void {
            mode = 2;
            clearStage();
            label("Game Over", 34, 0xFF6A6A, 120);
            label("Punkte: " + score, 24, 0xFFFFFF, 200);
            label("Leertaste = Menue", 18, 0x888888, 280);
        }

        private function update(event:Event):void {
            var now:int = getTimer();
            var dt:Number = (now - lastTime) / 1000;
            lastTime = now;
            if (dt <= 0 || dt > 0.1) {
                dt = 0.016;
            }
            Input.poll();

            if (mode == 0) {
                if (Input.pressed(Keyboard.UP)) {
                    selected = selected - 1;
                    if (selected < 0) { selected = items.length - 1; }
                }
                if (Input.pressed(Keyboard.DOWN)) {
                    selected = selected + 1;
                    if (selected >= items.length) { selected = 0; }
                }
                var i:int;
                for (i = 0; i < items.length; i++) {
                    var tf:TextField = items[i];
                    if (i == selected) {
                        tf.text = "> " + tf.text.substring(2);
                    } else {
                        tf.text = "  " + tf.text.substring(2);
                    }
                }
                if (Input.pressed(Keyboard.SPACE) || Input.pressed(Keyboard.ENTER)) {
                    startGame();
                }
            } else if (mode == 1) {
                var dx:Number = 0;
                var dy:Number = 0;
                if (Input.down(Keyboard.LEFT)) { dx = dx - 1; }
                if (Input.down(Keyboard.RIGHT)) { dx = dx + 1; }
                if (Input.down(Keyboard.UP)) { dy = dy - 1; }
                if (Input.down(Keyboard.DOWN)) { dy = dy + 1; }
                player.wx = player.wx + dx * 240 * dt;
                player.wy = player.wy + dy * 240 * dt;
                if (player.wx < 20) { player.wx = 20; }
                if (player.wx > stage.stageWidth - 20) { player.wx = stage.stageWidth - 20; }
                if (player.wy < 60) { player.wy = 60; }
                if (player.wy > stage.stageHeight - 20) { player.wy = stage.stageHeight - 20; }
                player.x = player.wx;
                player.y = player.wy;
                score = score + 1;
                if (Input.pressed(Keyboard.ESCAPE)) {
                    gameOver();
                }
            } else {
                if (Input.pressed(Keyboard.SPACE)) {
                    clearStage();
                    buildMenu();
                }
            }
        }
    }
}
