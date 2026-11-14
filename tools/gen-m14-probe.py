import json, uuid, sys

def uid():
    return str(uuid.uuid4())

src = r'''package {
    import flash.display.MovieClip;
    import flash.utils.Timer;
    import flash.events.TimerEvent;
    import flash.utils.ByteArray;
    import flash.utils.Dictionary;
    import flash.geom.Point;
    import flash.geom.Rectangle;
    import flash.events.Event;

    public dynamic class Main extends MovieClip {
        private var frame:int = 0;
        private var timer:Timer;
        private var ba:ByteArray;
        public function Main() {
            addEventListener("enterFrame", onTick);
        }
        private function onTick(e:Event):void {
            frame++;
            if (frame == 5) {
                var o = JSON.parse("{\"a\":1,\"b\":[2,3]}");
                trace("JSON a=" + o.a + " b1=" + o.b[1]);
                trace("JSON str=" + JSON.stringify({x:7}));
                ba = new ByteArray();
                ba.writeInt(258); ba.writeUTFBytes("hi");
                ba.position = 0;
                trace("BA int=" + ba.readInt() + " s=" + ba.readUTFBytes(2));
                timer = new Timer(50, 2);
                timer.addEventListener(TimerEvent.TIMER, onTimer);
                timer.start();
                var d = new Dictionary();
                d["k"] = 42;
                trace("Dict=" + d["k"]);
                var p = new Point(3, 4);
                var r = new Rectangle(0,0,10,10);
                trace("Pt=" + p.x + "," + p.y + " R=" + r.width);
            }
            if (frame == 30) {
                trace("M14PROBE DONE");
            }
        }
        private function onTimer(e:TimerEvent):void {
            trace("timer tick");
        }
    }
}
'''

proj = {"id": uid(), "metadata": {"name": "m14-probe", "author": "", "description": "", "createdUnix": 0, "modifiedUnix": 0},
        "scenes": [{"id": uid(), "name": "Szene 1", "width": 640.0, "height": 360.0, "background": {"r": 0, "g": 0, "b": 0, "a": 255}, "fps": 30.0, "layers": [{"id": uid(), "name": "Ebene 1", "visible": True, "locked": False, "kind": "normal", "frames": [{"keyframe": True, "objects": []}]}], "game": None}],
        "symbols": [], "assets": [], "tilesets": [],
        "scripts": [{"id": uid(), "name": "Main.as", "path": "scripts/Main.as", "source": src, "provenance": "original_source"}],
        "export": {"target": "avm2", "swfVersion": 10, "compression": "zlib", "includeScripts": True, "exportAllScenes": False}, "activeScene": None}
proj["activeScene"] = proj["scenes"][0]["id"]
env = {"format": "nova-flash-studio", "formatVersion": 4, "project": proj}
open("samples/m14-probe.nova", "w", encoding="utf-8").write(json.dumps(env, indent=1))
print("written")
