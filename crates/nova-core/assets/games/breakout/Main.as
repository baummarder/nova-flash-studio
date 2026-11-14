package {
    import flash.display.MovieClip;
    import flash.display.Graphics;
    import flash.events.Event;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import flash.ui.Keyboard;
    import flash.utils.getTimer;
    import game.Input;
    import game.Collision;
    import game.Random;

    /// Arkanoid-style breakout. Bricks, paddle and ball are drawn
    /// programmatically; lives and score live in a TextField.
    public dynamic class Main extends MovieClip {
        private var paddle:MovieClip;
        private var ball:MovieClip;
        private var hud:TextField;
        private var bricks = [];
        private var rows:int = 5;
        private var cols:int = 8;
        private var score:int = 0;
        private var lives:int = 3;
        private var lastTime:int = 0;
        private var stuck:Boolean = true;

        public function Main() {
            Input.init(stage);
            build();
            addEventListener(Event.ENTER_FRAME, update);
        }

        private function box(w:Number, h:Number, color:Number):MovieClip {
            var s:MovieClip = new MovieClip();
            var g:Graphics = s.graphics;
            g.beginFill(color);
            g.drawRect(-w / 2, -h / 2, w, h);
            g.endFill();
            return s;
        }

        private function build():void {
            var brickW:Number = (stage.stageWidth - 80) / cols;
            var brickH:Number = 22;
            for (var r:int = 0; r < rows; r++) {
                for (var c:int = 0; c < cols; c++) {
                    var hue:Number = 0x3366FF + r * 0x003300;
                    var b:MovieClip = box(brickW - 4, brickH - 4, hue);
                    b.x = 40 + brickW * (c + 0.5);
                    b.y = 60 + brickH * (r + 0.5);
                    addChild(b);
                    bricks.push(b);
                }
            }
            paddle = box(90, 14, 0xE8E8E8);
            paddle.x = stage.stageWidth / 2;
            paddle.y = stage.stageHeight - 40;
            addChild(paddle);
            ball = box(14, 14, 0xFFFFFF);
            ball.x = paddle.x;
            ball.y = paddle.y - 20;
            addChild(ball);
            hud = new TextField();
            var fmt:TextFormat = new TextFormat();
            fmt.size = 22;
            fmt.color = 0xFFFFFF;
            hud.defaultTextFormat = fmt;
            hud.x = 16;
            hud.y = 16;
            addChild(hud);
            updateHud();
        }

        private function updateHud():void {
            hud.text = "Punkte: " + score + "   Leben: " + lives;
        }

        private function serve():void {
            stuck = false;
            ball.vx = (Random.range(2) - 1) * 200;
            ball.vy = -300;
            if (ball.vx == 0) {
                ball.vx = 150;
            }
        }

        private function update(event:Event):void {
            var now:int = getTimer();
            var dt:Number = (now - lastTime) / 1000;
            lastTime = now;
            if (dt <= 0 || dt > 0.1) {
                dt = 0.016;
            }
            Input.poll();

            if (Input.down(Keyboard.LEFT)) {
                paddle.x = paddle.x - 380 * dt;
            }
            if (Input.down(Keyboard.RIGHT)) {
                paddle.x = paddle.x + 380 * dt;
            }
            if (paddle.x < 40 + paddle.width / 2) { paddle.x = 40 + paddle.width / 2; }
            if (paddle.x > stage.stageWidth - 40 - paddle.width / 2) { paddle.x = stage.stageWidth - 40 - paddle.width / 2; }

            if (stuck) {
                ball.x = paddle.x;
                ball.y = paddle.y - 20;
                if (Input.pressed(Keyboard.SPACE)) {
                    serve();
                }
                updateHud();
                return;
            }

            ball.x = ball.x + ball.vx * dt;
            ball.y = ball.y + ball.vy * dt;

            // Walls
            if (ball.x < 20 + ball.width / 2) {
                ball.x = 20 + ball.width / 2;
                ball.vx = -ball.vx;
            }
            if (ball.x > stage.stageWidth - 20 - ball.width / 2) {
                ball.x = stage.stageWidth - 20 - ball.width / 2;
                ball.vx = -ball.vx;
            }
            if (ball.y < 50 + ball.height / 2) {
                ball.y = 50 + ball.height / 2;
                ball.vy = -ball.vy;
            }

            // Paddle
            if (Collision.objectsOverlap(ball, paddle) && ball.vy > 0) {
                ball.vy = -ball.vy * 1.04;
                ball.vx = ball.vx + (ball.x - paddle.x) * 5;
                var maxV:Number = 560;
                if (ball.vx > maxV) { ball.vx = maxV; }
                if (ball.vx < -maxV) { ball.vx = -maxV; }
            }

            // Bricks
            var i:int = bricks.length - 1;
            while (i >= 0) {
                var b:MovieClip = bricks[i];
                if (b.parent != null && Collision.objectsOverlap(ball, b)) {
                    if (ball.vy > 0) { ball.vy = -ball.vy; }
                    else { ball.vy = -ball.vy; }
                    if (ball.vx > 0) { ball.vx = -ball.vx; }
                    else { ball.vx = -ball.vx; }
                    removeChild(b);
                    bricks.splice(i, 1);
                    score = score + 10;
                }
                i = i - 1;
            }

            // Miss
            if (ball.y > stage.stageHeight + 30) {
                lives = lives - 1;
                if (lives <= 0) {
                    hud.text = "Game Over! Punkte: " + score + "  (Leertaste = Neustart)";
                    stuck = true;
                    lives = 3;
                    score = 0;
                    rebuildBricks();
                } else {
                    stuck = true;
                    ball.vx = 0;
                    ball.vy = 0;
                }
            }
            updateHud();
        }

        private function rebuildBricks():void {
            // Bricks were removed permanently; rebuild only when needed.
            // For simplicity the level restarts with a fresh brick field.
            var brickW:Number = (stage.stageWidth - 80) / cols;
            var brickH:Number = 22;
            for (var r:int = 0; r < rows; r++) {
                for (var c:int = 0; c < cols; c++) {
                    var b:MovieClip = box(brickW - 4, brickH - 4, 0x3366FF + r * 0x003300);
                    b.x = 40 + brickW * (c + 0.5);
                    b.y = 60 + brickH * (r + 0.5);
                    addChild(b);
                    bricks.push(b);
                }
            }
        }
    }
}
