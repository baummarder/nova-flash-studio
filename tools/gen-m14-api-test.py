import json, uuid

def uid():
    return str(uuid.uuid4())

src = r'''package {
    import flash.display.MovieClip;
    import flash.display.Sprite;
    import flash.display.SimpleButton;
    import flash.display.Shape;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import flash.utils.Timer;
    import flash.events.TimerEvent;
    import flash.events.Event;
    import flash.utils.ByteArray;
    import flash.utils.Dictionary;
    import flash.utils.Endian;
    import flash.geom.Point;
    import flash.geom.Rectangle;
    import flash.geom.Matrix;
    import flash.geom.ColorTransform;
    import flash.geom.Transform;
    import flash.display.BitmapData;
    import flash.display.Bitmap;
    import flash.display.Graphics;
    import flash.display.LoaderInfo;

    public dynamic class Main extends MovieClip {
        private var frame:int = 0;
        private var results:Object = {};

        public function Main() {
            addEventListener("enterFrame", onTick);
        }

        private function ok(name:String, cond:Boolean):void {
            results[name] = cond;
            trace("M14 " + name + "=" + cond);
        }

        private function onTick(e:Event):void {
            frame++;
            if (frame == 5) {
                // --- JSON ---
                var o = JSON.parse("{\"a\":1,\"b\":[2,3],\"s\":\"x\"}");
                ok("json", o.a == 1 && o.b[1] == 3 && o.s == "x");
                var js = JSON.stringify({x: 7, y: [1, 2]});
                ok("jsonStr", js.indexOf("\"x\":7") >= 0);

                // --- ByteArray ---
                var ba = new ByteArray();
                ba.endian = Endian.LITTLE_ENDIAN;
                ba.writeInt(0x12345678);
                ba.writeUTFBytes("hi");
                ba.position = 0;
                ok("ba", ba.readInt() == 0x12345678 && ba.readUTFBytes(2) == "hi");
                ba.position = 0;
                ba.endian = Endian.BIG_ENDIAN;
                ok("baLen", ba.length == 6);

                // --- Dictionary / Point / Rectangle / Matrix / ColorTransform ---
                var d = new Dictionary();
                d["k"] = 42;
                ok("dict", d["k"] == 42);
                var p = new Point(3, 4);
                var p2 = p.clone();
                ok("point", p.x == 3 && p.y == 4 && p2.x == 3);
                var r = new Rectangle(1, 2, 10, 20);
                ok("rect", r.width == 10 && r.contains(5, 5) && !r.contains(0, 0));
                var m = new Matrix();
                m.tx = 5; m.ty = 6;
                var mp = m.transformPoint(new Point(0, 0));
                ok("matrix", mp.x == 5 && mp.y == 6);
                var ct = new ColorTransform();
                ct.redMultiplier = 0.5;
                ok("colorXform", ct.redMultiplier == 0.5);

                // --- String / Array methods ---
                var s = "Hello World";
                ok("str", s.charAt(0) == "H" && s.indexOf("World") == 6
                    && s.substr(0, 5) == "Hello" && s.toUpperCase() == "HELLO WORLD"
                    && s.split(" ").length == 2);
                var a = [3, 1, 2];
                a.push(4);
                a.sort();
                ok("array", a.length == 4 && a[0] == 1 && a[3] == 4
                    && a.indexOf(2) == 1 && a.join("-") == "1-2-3-4");

                // --- TextField (dynamic text) ---
                var tf = new TextField();
                tf.text = "NOVA";
                var fmt = new TextFormat();
                fmt.size = 24; fmt.color = 0xFF0000;
                tf.defaultTextFormat = fmt;
                addChild(tf);
                ok("text", tf.text == "NOVA" && tf.textWidth > 0);

                // --- Timer ---
                var timer = new Timer(40, 2);
                timer.addEventListener(TimerEvent.TIMER, onTimer);
                timer.start();

                // --- SimpleButton ---
                var btn = new SimpleButton();
                var up = new Sprite(); up.graphics.beginFill(0x336699); up.graphics.drawRect(0, 0, 60, 20); up.graphics.endFill();
                var over = new Sprite(); over.graphics.beginFill(0x66CCFF); over.graphics.drawRect(0, 0, 60, 20); over.graphics.endFill();
                var down = new Sprite(); down.graphics.beginFill(0x223344); down.graphics.drawRect(0, 0, 60, 20); down.graphics.endFill();
                var hit = new Sprite(); hit.graphics.beginFill(0xFFFFFF); hit.graphics.drawRect(0, 0, 60, 20); hit.graphics.endFill();
                btn.upState = up; btn.overState = over; btn.downState = down;
                btn.x = 100; btn.y = 100;
                addChild(btn);
                ok("button", btn.upState == up && btn.hitTestPoint(110, 110));

                // --- Graphics curve drawing ---
                var g = new Shape();
                g.graphics.lineStyle(2, 0x00FF00);
                g.graphics.moveTo(0, 0);
                g.graphics.lineTo(50, 0);
                g.graphics.curveTo(80, 40, 100, 0);
                g.graphics.endFill();
                g.x = 200; g.y = 200;
                addChild(g);
                ok("graphics", true);

                // --- BitmapData ---
                var bd = new BitmapData(8, 8, false, 0xFF0000);
                bd.fillRect(new Rectangle(0, 0, 4, 8), 0x00FF00);
                ok("bitmapData", bd.getPixel(1, 1) == 0x00FF00);
                var bmp = new Bitmap(bd);
                addChild(bmp);
                bmp.x = 400; bmp.y = 100;

                // --- LoaderInfo via loaderInfo property ---
                ok("loaderInfo", loaderInfo != null && loaderInfo.width == 640);
            }
            if (frame == 40) {
                trace("M14 ALL=true");
                trace("M14DONE");
            }
        }
        private function onTimer(e:TimerEvent):void {
            trace("M14 timer=" + e.target.repeatCount);
        }
    }
}
'''

proj = {"id": uid(), "metadata": {"name": "M14 API Surface", "author": "", "description": "", "createdUnix": 0, "modifiedUnix": 0},
        "scenes": [{"id": uid(), "name": "Szene 1", "width": 640.0, "height": 360.0, "background": {"r": 18, "g": 20, "b": 26, "a": 255}, "fps": 30.0, "layers": [{"id": uid(), "name": "Ebene 1", "visible": True, "locked": False, "kind": "normal", "frames": [{"keyframe": True, "objects": []}]}], "game": None}],
        "symbols": [], "assets": [], "tilesets": [],
        "scripts": [{"id": uid(), "name": "Main.as", "path": "scripts/Main.as", "source": src, "provenance": "original_source"}],
        "export": {"target": "avm2", "swfVersion": 13, "compression": "zlib", "includeScripts": True, "exportAllScenes": False}, "activeScene": None}
proj["activeScene"] = proj["scenes"][0]["id"]
env = {"format": "nova-flash-studio", "formatVersion": 4, "project": proj}
open("samples/m14-api-surface.nova", "w", encoding="utf-8").write(json.dumps(env, indent=1))
print("written")
