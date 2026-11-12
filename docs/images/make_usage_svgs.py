#!/usr/bin/env python3
"""README の「使い方」に貼る、画面の動きの SVG (docs/images/usage-*.svg) を作る。

- 場面ごとに 1 枚。JavaScript は使わず、SVG の中の CSS の @keyframes だけで繰り返し動く
  (GitHub の README の画像では JavaScript が動かないため)
- 外のフォントは読み込めないので、見る側の OS の日本語のフォントで出す
- 背景を SVG の中に持たせ、GitHub の明るい色と暗い色のどちらでも読めるようにする
- 動きを減らす設定 (prefers-reduced-motion: reduce) では、場面の終わりの姿で止める

文言や配置を直すときは、SVG を手で直さず、この script を直して作り直す。標準ライブラリだけを使う。

    python3 docs/images/make_usage_svgs.py
"""

from __future__ import annotations

from dataclasses import dataclass, replace
from pathlib import Path
from typing import Callable

OUT_DIR = Path(__file__).resolve().parent

# ---- 台の大きさ・字・色 ----

# type-clipboard の窓のメニューの帯の高さ (「ヘルプ」だけ。src/win/ui.rs の menu_bar)。台と窓は、この分だけ高くする
MENU_H = 20

W, H = 920, 640 + MENU_H
TASKBAR_Y = 612 + MENU_H

FONT = (
    '"Yu Gothic UI", "Meiryo UI", Meiryo, "Hiragino Sans", "Hiragino Kaku Gothic ProN", '
    '"Noto Sans CJK JP", "Noto Sans JP", "Segoe UI", system-ui, sans-serif'
)
MONO = '"Cascadia Mono", Consolas, "DejaVu Sans Mono", "Noto Sans Mono", ui-monospace, monospace'

# アプリの色は src/win/ui.rs に合わせる
INK = "#1a1a1a"
MUTED = "#6b6b6b"
LINE = "#c9ced6"
ORANGE = "#f59e0b"  # 候補
BLUE = "#2563eb"  # 打つ先・打っている
GREEN = "#15803d"  # 打ち終わり
AMBER = "#b45309"  # 止めた
RED = "#ef4444"  # 札の点
PANEL = "#fafafa"  # プレビューの欄の地
TITLEBAR = "#eef1f5"
SELECTED = "#0078d7"  # 一覧の選んだ行 (一覧に focus があるとき)
SELECTED_INACTIVE = "#e1e5ea"  # 一覧の選んだ行 (focus が無いとき)
BAR = "#06b025"  # 進み具合のバー
BAR_PAUSED = "#d9a400"  # 止めたときの進み具合のバー

TOTAL = 60  # 打つ文字数 (パスワード管理アプリでコピーした値)
DT = 0.05  # 1 文字ごとの間 (秒)。実際 (30 ms) より遅くして見やすくする
DOTS = 24  # パスワードの欄に見える伏せ字の数 (それより後は欄の中で送られる)

STEP = "step-end"
MOVE = "cubic-bezier(.4,.1,.2,1)"  # マウスの動き
HIDDEN, SHOWN = "opacity:0", "opacity:1"


def vis(on: bool) -> str:
    return SHOWN if on else HIDDEN


def num(v: float) -> str:
    return f"{v:.3f}".rstrip("0").rstrip(".")


def esc(s: str) -> str:
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def text(x: float, y: float, s: str, cls: str = "", anchor: str = "", attr: str = "") -> str:
    if cls and ' class="' in attr:
        # 動きの class (Motion.attr) と字の class を 1 つの属性にまとめる
        attr, cls = attr.replace(' class="', f' class="{cls} ', 1), ""
    a = f' text-anchor="{anchor}"' if anchor else ""
    c = f' class="{cls}"' if cls else ""
    return f'<text x="{num(x)}" y="{num(y)}"{c}{a}{attr}>{esc(s)}</text>'


# ---- 机の上の窓 (例の名前だけを使う) ----


@dataclass(frozen=True)
class Win:
    id: str
    x: int
    y: int
    w: int
    h: int
    title: str
    exe: str


NOTE = Win("note", 18, 18, 280, 176, "メモ.txt - メモ帳", "notepad.exe")
VNC = Win("vnc", 178, 96, 414, 262, "vm02 - Console - Browser", "browser.exe")
VM = Win("vm", 30, 236, 430, 290, "vm01 - 仮想マシン接続", "vmconnect.exe")
WINDOWS = {w.id: w for w in (NOTE, VNC, VM)}
Z_ORDER = ("vm", "vnc", "note")  # 上から

# ---- type-clipboard の窓 (src/win/ui.rs の並びを縮めたもの) ----

APP_X, APP_Y, APP_W, APP_H = 612, 8, 298, 596 + MENU_H
TITLE_H = 28
CX, CR = APP_X + 10, APP_X + APP_W - 10  # 中身の左右
CW = CR - CX
# 中身はメニューの帯の下から並べる
REFRESH = (752, 45 + MENU_H, 30, 24)  # x, y, w, h
PICK = (788, 45 + MENU_H, 112, 24)
LIST_Y, LIST_H, HEADER_H, ROW_H = 75 + MENU_H, 106, 22, 20
EXE_X = 802  # 一覧の exe の列の左
ROWS = (
    ("vm", VM.title, VM.exe),
    ("vnc", VNC.title, VNC.exe),
    ("note", NOTE.title, NOTE.exe),
    ("", "ダウンロード - エクスプローラー", "explorer.exe"),
)
PANEL_Y, PANEL_H = 187 + MENU_H, 146
# サムネイルを置く所: 欄から 8 px 内側で、下は札の帯を空ける (src/win/app.rs の thumbnail_area)
THUMB_X, THUMB_Y, THUMB_W, THUMB_H = CX + 8, PANEL_Y + 8, CW - 16, PANEL_H - 40
TYPE_BTN = (CX, 520 + MENU_H, 174, 28)
TRIAL_BTN = (802, 520 + MENU_H, 98, 28)
BUTTONS = {"pick": PICK, "type": TYPE_BTN}

# vm01 の中: 画面 (メニューの帯の下) とパスワードの欄の左上 (窓の左上からの位置)
VM_SCREEN_Y = TITLE_H + 22
PW_X, PW_Y = VM.w / 2 - 100, VM_SCREEN_Y + 118

# 机の上の点
PW_BOX = (VM.x + PW_X, VM.y + PW_Y)
SPOT = {
    "pick": (PICK[0] + PICK[2] / 2, PICK[1] + PICK[3] / 2),
    "type": (TYPE_BTN[0] + TYPE_BTN[2] / 2, TYPE_BTN[1] + TYPE_BTN[3] / 2),
    "note": (110, 92),
    "vnc": (520, 160),
    "vm": (300, 330),
    "pw": (PW_BOX[0] + 100, PW_BOX[1] + 14),
}
TOAST = (18, 548 + MENU_H, 236, 36)
KEYCAP = (478, 470)

# ---- 状態 ----


@dataclass(frozen=True)
class State:
    app: bool = True  # type-clipboard の窓が出ている
    cand: str = ""  # 候補 (一覧でなぞっている・クリックで選ぶの最中に指している窓)
    target: str = ""  # 打つ先
    flash: bool = False  # 決めた直後に打つ先へ出す青の枠 (0.7 秒)
    list_focus: bool = False  # 一覧が focus を持つ (選んだ行が青)
    picking: bool = False  # 「クリックで選ぶ」の最中
    status: tuple = ("choose",)
    clip: int = 0  # クリップボードの文字数
    pw_focus: bool = False  # vm01 のパスワードの欄が focus を持つ
    typed: int = 0  # 打った文字数
    welcome: bool = False  # vm01 がサインインした
    toast: bool = False
    keycap: bool = False
    note_front: bool = False  # メモ帳が前に来た
    busy: bool = False  # 打っている間 (入力の部品を使えない)
    progress: tuple = (0, "normal")


def shown(s: State) -> str:
    """プレビューに映す窓。"""
    return s.cand or s.target


def role(s: State) -> str:
    """プレビューの札 (src/win/app.rs の role)。"""
    if s.cand and s.cand != s.target:
        return "cand"
    return "target" if s.target else "empty"


def frame(s: State) -> tuple:
    """机の上の枠 (src/win/app.rs の refresh_preview と flash_target)。"""
    if s.cand:
        return (s.cand, "blue" if s.cand == s.target else "orange")
    if s.flash and s.target:
        return (s.target, "blue")
    return ()


STATUS = {
    "choose": ("打つ先を選んでください", "", "一覧から選ぶか「クリックで選ぶ」"),
    "picking": ("選びたい窓をクリック", "", "Esc でやめる"),
    "decided": ("打つ先を決めた", "", "欄を出し、コピーしてから「打つ」"),
    "typing": ("打っています {n} / {total}", "c-blue", "キーを押すか、クリックすると止まる"),
    "done": ("{total} 文字を打ちました", "c-green", "Enter は打つ先で自分で押す"),
    "stopped": ("止めました ({n} / {total})", "c-amber", "クリックがあったため"),
}

# ---- 場面 ----


@dataclass
class Scene:
    name: str
    title: str
    desc: str  # README の alt と同じ文
    duration: float
    base: State
    cursor: tuple
    moves: tuple = ()  # (始まり, 終わり, 行き先)
    clicks: tuple = ()  # 押した時刻
    presses: tuple = ()  # (時刻, 押したボタン)
    events: tuple = ()  # (時刻, 変わる状態)
    hover: str = ""  # "list": 一覧の行をなぞる / "pick": クリックで選ぶの最中に窓を指す


def typing(start: float, upto: int) -> list:
    """`start` から 1 文字ずつ打つ (`upto` 文字目まで)。"""
    return [
        (start + (i - 1) * DT, {"typed": i, "status": ("typing", i), "progress": (i, "normal")})
        for i in range(1, upto + 1)
    ]


DECIDED = State(target="vm", status=("decided",))
READY = replace(DECIDED, clip=TOTAL, pw_focus=True)
TYPE_START = 1.5  # 「クリップボードを打つ」を押してから 0.5 秒置く

SCENES = (
    Scene(
        "usage-1-launch",
        "type-clipboard を起こした直後",
        "type-clipboard を起こした直後の画面。メモ帳・ブラウザの VNC のコンソール (vm02)・"
        "仮想マシン接続 (vm01) の窓の右に、type-clipboard の窓が出る。打つ先はまだ無く、プレビューの欄は空のまま。",
        4.0,
        State(app=False),
        (540, 560),
        events=((0.5, {"app": True}),),
    ),
    Scene(
        "usage-2-list",
        "一覧で打つ先を選ぶ",
        "一覧の行をなぞると、その窓が「候補」として橙の枠でプレビューに映り、画面の上でもその窓に橙の枠が付く。"
        "vm01 の行をクリックすると打つ先に決まり、プレビューの札が青の「打つ先」になる。",
        7.0,
        State(),
        (602, 150 + MENU_H),
        moves=((0.6, 1.2, (690, 150 + MENU_H)), (2.0, 2.3, (690, 128 + MENU_H)), (3.1, 3.4, (690, 108 + MENU_H))),
        clicks=(4.1,),
        events=(
            (4.1, {"target": "vm", "cand": "", "flash": True, "list_focus": True, "status": ("decided",)}),
            (4.8, {"flash": False}),
        ),
        hover="list",
    ),
    Scene(
        "usage-3-pick",
        "クリックで打つ先を選ぶ",
        "「クリックで選ぶ」を押すと、ボタンが「やめる (Esc)」に変わり、マウスで指した窓が候補として映る。"
        "vm01 をクリックすると打つ先に決まる。そのクリックは vm01 には届かない。",
        8.0,
        State(),
        (602, 57 + MENU_H),  # 一覧の上を通ると候補が映るので、ボタンの行を横に進む
        moves=(
            (0.4, 1.1, SPOT["pick"]),
            (1.5, 2.4, SPOT["note"]),
            (3.0, 3.8, SPOT["vnc"]),
            (4.4, 5.2, SPOT["vm"]),
        ),
        clicks=(1.1, 5.8),
        presses=((1.1, "pick"),),
        events=(
            (1.1, {"picking": True, "status": ("picking",)}),
            (5.8, {"picking": False, "cand": "", "target": "vm", "flash": True, "status": ("decided",)}),
            (6.5, {"flash": False}),
        ),
        hover="pick",
    ),
    Scene(
        "usage-4-copy",
        "パスワードの欄を出してコピーする",
        "vm01 のパスワードの欄をクリックすると、プレビューにも欄の様子が映る。"
        "パスワード管理アプリでコピーすると、type-clipboard には「クリップボード: 60 文字」と文字数だけが出る。",
        6.5,
        DECIDED,
        SPOT["vm"],
        moves=((0.4, 1.1, SPOT["pw"]),),
        clicks=(1.1,),
        events=(
            (1.1, {"pw_focus": True}),
            (1.8, {"toast": True}),
            (2.4, {"clip": TOTAL}),
            (4.8, {"toast": False}),
        ),
    ),
    Scene(
        "usage-5-type",
        "クリップボードを打つ",
        "「クリップボードを打つ」を押すと、0.5 秒ほど置いてから vm01 のパスワードの欄に 1 文字ずつ打ち込まれ、"
        "type-clipboard には「打っています n / 60」と進み具合だけが出る。打ち終わると「60 文字を打ちました」と出る。",
        7.0,
        READY,
        SPOT["pw"],
        moves=((0.3, 1.0, SPOT["type"]),),
        clicks=(1.0,),
        presses=((1.0, "type"),),
        events=(
            (1.0, {"status": ("typing", 0), "busy": True, "progress": (0, "normal")}),
            *typing(TYPE_START, TOTAL),
            (TYPE_START + TOTAL * DT, {"status": ("done",), "busy": False}),
        ),
    ),
    Scene(
        "usage-6-enter",
        "打ち終わったら Enter を押す",
        "打ち終わった後、利用者が Enter を押すと vm01 にサインインする。type-clipboard は Enter を押さない。",
        5.5,
        replace(READY, typed=TOTAL, status=("done",), progress=(TOTAL, "normal")),
        SPOT["type"],
        moves=((0.4, 1.2, (300, 470)),),
        events=(
            (1.5, {"keycap": True}),
            (1.9, {"pw_focus": False, "welcome": True}),
            (3.9, {"keycap": False}),
        ),
    ),
    Scene(
        "usage-7-stop",
        "打っている最中に止まる",
        "打っている最中にメモ帳をクリックすると、そこで止まり、「止めました (23 / 60)」と何文字打ったところで止めたかが出る。",
        7.0,
        READY,
        SPOT["pw"],
        moves=((0.3, 1.0, SPOT["type"]), (1.65, 2.45, SPOT["note"])),
        clicks=(1.0, 2.62),
        presses=((1.0, "type"),),
        events=(
            (1.0, {"status": ("typing", 0), "busy": True, "progress": (0, "normal")}),
            *typing(TYPE_START, 23),
            (2.62, {"status": ("stopped", 23), "busy": False, "progress": (23, "paused"), "note_front": True}),
        ),
    ),
)

# ---- マウスの動きとなぞった先 ----


def bezier(x1: float, y1: float, x2: float, y2: float) -> Callable[[float], float]:
    """CSS の cubic-bezier と同じ緩急。"""

    def curve(a: float, b: float, u: float) -> float:
        return 3 * a * u * (1 - u) ** 2 + 3 * b * u * u * (1 - u) + u**3

    def ease(x: float) -> float:
        lo, hi = 0.0, 1.0
        for _ in range(40):
            mid = (lo + hi) / 2
            if curve(x1, x2, mid) < x:
                lo = mid
            else:
                hi = mid
        return curve(y1, y2, (lo + hi) / 2)

    return ease


EASE_MOVE = bezier(0.4, 0.1, 0.2, 1)


def cursor_at(sc: Scene, t: float) -> tuple:
    pos = sc.cursor
    for start, end, to in sc.moves:
        if t < start:
            return pos
        if t <= end:
            k = EASE_MOVE((t - start) / (end - start))
            return (pos[0] + (to[0] - pos[0]) * k, pos[1] + (to[1] - pos[1]) * k)
        pos = to
    return pos


def inside(x: float, y: float, rx: float, ry: float, rw: float, rh: float) -> bool:
    return rx <= x < rx + rw and ry <= y < ry + rh


def window_at(x: float, y: float) -> str:
    """クリックで選べる窓 (自分の窓・デスクトップは選べない)。"""
    if inside(x, y, APP_X, APP_Y, APP_W, APP_H):
        return ""
    for wid in Z_ORDER:
        w = WINDOWS[wid]
        if inside(x, y, w.x, w.y, w.w, w.h):
            return wid
    return ""


def row_at(x: float, y: float) -> str:
    """一覧の行の窓 (行の外なら空)。"""
    top = LIST_Y + HEADER_H
    if not inside(x, y, CX, top, CW, ROW_H * len(ROWS)):
        return ""
    return ROWS[int((y - top) // ROW_H)][0]


def simulate(sc: Scene) -> list:
    """時刻と状態の組を、状態が変わるところだけ並べる。"""
    samples = set()
    if sc.hover:
        for start, end, _ in sc.moves:
            n = round((end - start) / 0.01)
            samples.update(round(start + i * 0.01, 4) for i in range(n + 1))
    events = sorted(sc.events, key=lambda e: e[0])
    times = sorted(samples | {round(t, 4) for t, _ in events})
    state, snaps, k = sc.base, [(0.0, sc.base)], 0
    for t in times:
        new = state
        while k < len(events) and round(events[k][0], 4) <= t:
            new = replace(new, **events[k][1])
            k += 1
        if t in samples:
            x, y = cursor_at(sc, t)
            if sc.hover == "list" and not new.busy:
                new = replace(new, cand=row_at(x, y))
            elif sc.hover == "pick" and new.picking:
                new = replace(new, cand=window_at(x, y))
        if new != state:
            if snaps[-1][0] == t:
                snaps[-1] = (t, new)
            else:
                snaps.append((t, new))
            state = new
    return snaps


# ---- 動き (CSS の @keyframes) ----


class Motion:
    """1 枚の SVG の動き。どれも場面の長さで繰り返す。

    要素の地の姿は場面の終わりの姿にする。動きを減らす設定で animation を外すと、その姿で止まる。
    """

    def __init__(self, sc: Scene) -> None:
        self.d = sc.duration
        self.snaps = simulate(sc)
        self.rules: list = []
        self.names: dict = {}

    def timeline(self, fn: Callable[[State], str]) -> list:
        tl: list = []
        for t, s in self.snaps:
            v = fn(s)
            if not tl or tl[-1][1] != v:
                tl.append((t, v))
        return tl

    def never(self, fn: Callable[[State], str]) -> bool:
        """一度も出ない要素 (描かなくてよい)。"""
        return self.timeline(fn) == [(0.0, HIDDEN)]

    def attr(self, fn: Callable[[State], str], ease: tuple = (), cls: str = "") -> str:
        """状態から CSS の宣言を返す `fn` で動かす要素の属性。`ease` は (秒, 緩急) で、変わるときに滑らかに移す。"""
        tl = self.timeline(fn)
        if len(tl) == 1:
            c = f' class="{cls}"' if cls else ""
            return f'{c} style="{tl[0][1]}"'
        stops, prev = [], ""
        for i, (t, v) in enumerate(tl):
            if i == 0:
                stops.append((0.0, v, STEP))
            elif ease:
                until = tl[i + 1][0] if i + 1 < len(tl) else self.d
                stops.append((t, prev, ease[1]))
                stops.append((t + min(ease[0], until - t), v, STEP))
            else:
                stops.append((t, v, STEP))
            prev = v
        return self.custom(stops, cls)

    def custom(self, stops: list, cls: str = "") -> str:
        """(時刻, 宣言, 次までの緩急) の並びから @keyframes を作る。"""
        merged: list = []
        for t, v, timing in stops:
            if merged and abs(merged[-1][0] - t) < 1e-6:
                merged[-1] = (t, v, timing)
            else:
                merged.append((t, v, timing))
        if merged[-1][0] < self.d - 1e-6:
            merged.append((self.d, merged[-1][1], ""))
        body = "".join(
            f"{num(t / self.d * 100)}%{{{v}" + (f";animation-timing-function:{timing}" if timing else "") + "}"
            for t, v, timing in merged
        )
        name = self.names.get(body)
        if name is None:
            name = f"a{len(self.names) + 1}"
            self.names[body] = name
            self.rules.append(f"@keyframes {name}{{{body}}}")
            self.rules.append(f".{name}{{{merged[-1][1]};animation:{name} {num(self.d)}s infinite}}")
        return f' class="{(cls + " " + name).strip()}"'

    def pulse(self, at: float, on: str, off: str, length: float, timing: str = STEP, cls: str = "") -> str:
        """`at` に `on` になり、`length` 秒かけて `off` に戻る。"""
        return self.custom([(0.0, off, STEP), (at, on, timing), (at + length, off, STEP)], cls)


# ---- 描く部品 ----


def caption_buttons(w: float) -> str:
    """窓の右上の最小化・最大化・閉じる。"""
    y = TITLE_H / 2
    a, b, c = w - 66, w - 42, w - 18
    return (
        '<g stroke="#555" stroke-width="1.1" fill="none">'
        f'<line x1="{num(a - 4.5)}" y1="{y}" x2="{num(a + 4.5)}" y2="{y}"/>'
        f'<rect x="{num(b - 4.5)}" y="{y - 4.5}" width="9" height="9"/>'
        f'<path d="M{num(c - 4.5)},{y - 4.5}l9,9M{num(c + 4.5)},{y - 4.5}l-9,9"/></g>'
    )


def chrome(win: Win, body: list, shadow: bool) -> str:
    """窓の枠と題の帯。中身は窓の左上を原点にして描く。"""
    out = []
    if shadow:
        out.append(f'<rect width="{win.w}" height="{win.h}" rx="8" fill="#fff" filter="url(#shadow)"/>')
    out.append(f'<g clip-path="url(#clip-{win.id})">')
    out.append(f'<rect width="{win.w}" height="{win.h}" fill="#fff"/>')
    out.append(f'<rect width="{win.w}" height="{TITLE_H}" fill="{TITLEBAR}"/>')
    out.append(f'<line x1="0" y1="{TITLE_H - 0.5}" x2="{win.w}" y2="{TITLE_H - 0.5}" stroke="{LINE}"/>')
    out.append(text(10, 18.5, win.title, "t12"))
    out.append(caption_buttons(win.w))
    out.extend(body)
    out.append("</g>")
    out.append(f'<rect x=".5" y=".5" width="{win.w - 1}" height="{win.h - 1}" rx="7.5" fill="none" stroke="{LINE}"/>')
    return "".join(out)


def menu_bar(win: Win, items: tuple) -> list:
    out = [f'<line x1="0" y1="{TITLE_H + 21.5}" x2="{win.w}" y2="{TITLE_H + 21.5}" stroke="#e2e2e2"/>']
    x = 10
    for item in items:
        out.append(text(x, TITLE_H + 15, item, "t11 mu"))
        # かなを詰めるフォント (Yu Gothic UI) と詰めないフォントの間を取る
        x += len(item) * 10 + 14
    return out


def note_body(m: Motion) -> list:
    out = menu_bar(NOTE, ("ファイル", "編集", "表示"))
    lines = ("やること", "・VM の更新を当てる", "・コンソールから設定を確かめる", "・バックアップを取る")
    for i, line in enumerate(lines):
        out.append(text(12, TITLE_H + 42 + i * 20, line, "t12"))
    return out


def vnc_body(m: Motion) -> list:
    w, h = VNC.w, VNC.h
    out = [
        f'<rect x="8" y="{TITLE_H + 6}" width="{w - 16}" height="20" rx="10" fill="{TITLEBAR}"/>',
        text(20, TITLE_H + 20, "https://console.example/vm02", "t11 mu"),
        f'<rect x="8" y="{TITLE_H + 32}" width="{w - 16}" height="{h - TITLE_H - 40}" rx="4" fill="#0b0d10"/>',
    ]
    for i, line in enumerate(("Ubuntu 24.04 LTS vm02 tty1", "", "vm02 login: admin", "Password:")):
        if line:
            out.append(text(18, TITLE_H + 54 + i * 19, line, "t12 mono term"))
    out.append(f'<rect class="blink" x="90" y="{TITLE_H + 54 + 3 * 19 - 11}" width="7" height="13" fill="#c8d1dc"/>')
    return out


def vm_body(m: Motion) -> list:
    w, h = VM.w, VM.h
    top = VM_SCREEN_Y
    cx = w / 2
    bx, by = PW_X, PW_Y
    out = menu_bar(VM, ("ファイル", "操作", "メディア", "表示", "ヘルプ"))
    out.append(f'<rect y="{top}" width="{w}" height="{h - top}" fill="url(#vm-screen)"/>')
    out.append(f"<g{m.attr(lambda s: vis(not s.welcome), ease=(0.5, 'ease'))}>")
    out.append(f'<circle cx="{cx}" cy="{top + 60}" r="26" fill="#fff" fill-opacity=".85"/>')
    out.append(text(cx, top + 106, "user", "t14 b wh", "middle"))
    out.append(f'<rect x="{bx}" y="{by}" width="200" height="28" rx="4" fill="#fff" fill-opacity=".95"/>')
    focus = m.attr(lambda s: vis(s.pw_focus))
    out.append(
        f'<rect x="{bx + 1}" y="{by + 1}" width="198" height="26" rx="3.5" fill="none" stroke="{BLUE}" stroke-width="2"{focus}/>'
    )
    out.append(text(bx + 9, by + 18.5, "パスワード", "t13 ph", attr=m.attr(lambda s: vis(not s.pw_focus and not s.typed))))
    for i in range(DOTS):
        dot = m.attr(lambda s, i=i: vis(s.typed > i))
        out.append(f'<circle cx="{num(bx + 12 + i * 7.5)}" cy="{by + 14}" r="2.6" fill="#111"{dot}/>')
    caret = m.attr(lambda s: f"transform:translateX({num(min(s.typed, DOTS) * 7.5)}px)")
    out.append(
        f"<g{m.attr(lambda s: vis(s.pw_focus))}><g{caret}>"
        f'<rect class="blink" x="{bx + 9}" y="{by + 6}" width="1.4" height="16" fill="#111"/></g></g>'
    )
    out.append("</g>")
    out.append(text(cx, top + 128, "ようこそ", "t22 b wh", "middle", attr=m.attr(lambda s: vis(s.welcome), ease=(0.5, "ease"))))
    return out


BODIES = {"note": note_body, "vnc": vnc_body, "vm": vm_body}


def window(m: Motion, win: Win, shadow: bool = True) -> str:
    return chrome(win, BODIES[win.id](m), shadow)


def desk(m: Motion, sc: Scene) -> list:
    out = [f'<rect width="{W}" height="{H}" fill="url(#desk)"/>']
    for wid in reversed(Z_ORDER):
        win = WINDOWS[wid]
        out.append(f'<g transform="translate({win.x},{win.y})">{window(m, win)}</g>')
    front = lambda s: vis(s.note_front)  # noqa: E731
    if not m.never(front):
        out.append(f'<g transform="translate({NOTE.x},{NOTE.y})"><g{m.attr(front)}>{window(m, NOTE)}</g></g>')
    # 候補 (橙) と打つ先 (青) の枠。窓の内側の縁に描く (src/win/frame.rs)
    for wid, win in WINDOWS.items():
        for color_name, color in (("orange", ORANGE), ("blue", BLUE)):
            fn = lambda s, wid=wid, c=color_name: vis(frame(s) == (wid, c))  # noqa: E731
            if m.never(fn):
                continue
            out.append(
                f'<rect x="{win.x + 2}" y="{win.y + 2}" width="{win.w - 4}" height="{win.h - 4}" rx="6.5" '
                f'fill="none" stroke="{color}" stroke-width="4"{m.attr(fn)}/>'
            )
    # タスクバー
    out.append(f'<rect y="{TASKBAR_Y}" width="{W}" height="{H - TASKBAR_Y}" fill="#141820" fill-opacity=".82"/>')
    for i in range(5):
        out.append(f'<rect x="{W / 2 - 58 + i * 24}" y="{TASKBAR_Y + 6}" width="16" height="16" rx="4" fill="#fff" fill-opacity=".55"/>')
    out.append(text(W - 14, TASKBAR_Y + 18.5, "12:00", "t11 wh", "end"))
    return out


def button(box: tuple, label: str = "") -> str:
    x, y, w, h = box
    out = f'<rect x="{x + 0.5}" y="{y + 0.5}" width="{w - 1}" height="{h - 1}" rx="4" fill="#fdfdfd" stroke="#b4b9c1"/>'
    if label:
        out += text(x + w / 2, y + h / 2 + 4.5, label, "t13", "middle")
    return out


def refresh_icon(box: tuple) -> str:
    """↻ (字は OS のフォントで出ないことがあるので形で描く)。"""
    x, y, w, h = box
    cx, cy, r = x + w / 2, y + h / 2, 5.5
    return (
        f'<rect x="{x + 0.5}" y="{y + 0.5}" width="{w - 1}" height="{h - 1}" rx="4" fill="#fdfdfd" stroke="#b4b9c1"/>'
        f'<path d="M{num(cx + r)},{num(cy)}A{r},{r} 0 1 1 {num(cx + r * 0.5)},{num(cy - r * 0.87)}" '
        f'fill="none" stroke="{INK}" stroke-width="1.4"/>'
        f'<path d="M{num(cx + r * 0.5 - 2.6)},{num(cy - r * 0.87 - 2.6)}l3.4,1.6l-1.6,3.4" fill="none" stroke="{INK}" stroke-width="1.4"/>'
    )


def press(m: Motion, sc: Scene, name: str) -> str:
    """押したボタンを一瞬暗くする。"""
    x, y, w, h = BUTTONS[name]
    out = ""
    for t, which in sc.presses:
        if which == name:
            out += f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="4" fill="#000"{m.pulse(t, "opacity:.16", HIDDEN, 0.18)}/>'
    return out


def thumbnail(m: Motion, win: Win) -> str:
    """DWM のサムネイルの代わり: 窓を縮めて欄に収める (縦横比を保つ)。"""
    s = min(THUMB_W / win.w, THUMB_H / win.h, 1)
    tx = THUMB_X + (THUMB_W - win.w * s) / 2
    ty = THUMB_Y + (THUMB_H - win.h * s) / 2
    fn = lambda st: vis(shown(st) == win.id)  # noqa: E731
    if m.never(fn):
        return ""
    return f'<g transform="translate({num(tx)},{num(ty)}) scale({num(s)})"><g{m.attr(fn)}>{window(m, win, shadow=False)}</g></g>'


def app(m: Motion, sc: Scene) -> str:
    busy = m.attr(lambda s: "opacity:.45" if s.busy else SHOWN)
    out = [f'<rect x="{APP_X}" y="{APP_Y}" width="{APP_W}" height="{APP_H}" rx="8" fill="#fff" filter="url(#shadow)"/>']
    out.append('<g clip-path="url(#clip-app)">')
    out.append(f'<rect x="{APP_X}" y="{APP_Y}" width="{APP_W}" height="{TITLE_H}" fill="{TITLEBAR}"/>')
    out.append(f'<line x1="{APP_X}" y1="{APP_Y + TITLE_H - 0.5}" x2="{APP_X + APP_W}" y2="{APP_Y + TITLE_H - 0.5}" stroke="{LINE}"/>')
    out.append(text(CX, APP_Y + 18.5, "type-clipboard", "t12"))
    out.append(f'<g transform="translate({APP_X},{APP_Y})">{caption_buttons(APP_W)}</g>')
    # メニューの帯 (「ヘルプ」→「バージョン情報」。打っている間も使えるので薄くしない)
    out.append(text(CX, APP_Y + TITLE_H + 14, "ヘルプ(H)", "t12"))
    menu_line = APP_Y + TITLE_H + MENU_H - 0.5
    out.append(f'<line x1="{APP_X}" y1="{menu_line}" x2="{APP_X + APP_W}" y2="{menu_line}" stroke="#e2e2e2"/>')
    out.append("</g>")
    out.append(f'<rect x="{APP_X + 0.5}" y="{APP_Y + 0.5}" width="{APP_W - 1}" height="{APP_H - 1}" rx="7.5" fill="none" stroke="{LINE}"/>')

    # 打つ先・↻・クリックで選ぶ
    out.append(text(CX, PICK[1] + 17, "打つ先", "t13 b"))
    pick_x, pick_y = PICK[0] + PICK[2] / 2, PICK[1] + 16.5
    out.append(f"<g{busy}>{refresh_icon(REFRESH)}")
    out.append(button(PICK))
    out.append(text(pick_x, pick_y, "クリックで選ぶ", "t13", "middle", m.attr(lambda s: vis(not s.picking))))
    if not m.never(lambda s: vis(s.picking)):
        out.append(text(pick_x, pick_y, "やめる (Esc)", "t13", "middle", m.attr(lambda s: vis(s.picking))))
    out.append(press(m, sc, "pick") + "</g>")

    # 窓の一覧
    head = LIST_Y + HEADER_H
    out.append(f"<g{busy}>")
    out.append(f'<rect x="{CX + 0.5}" y="{LIST_Y + 0.5}" width="{CW - 1}" height="{LIST_H - 1}" fill="#fff" stroke="#a6abb3"/>')
    out.append(f'<line x1="{CX + 1}" y1="{head - 0.5}" x2="{CR - 1}" y2="{head - 0.5}" stroke="#e5e5e5"/>')
    out.append(f'<line x1="{EXE_X - 0.5}" y1="{LIST_Y + 4}" x2="{EXE_X - 0.5}" y2="{head - 4}" stroke="#e5e5e5"/>')
    out.append(text(CX + 6, LIST_Y + 15, "窓の題", "t12"))
    out.append(text(EXE_X + 6, LIST_Y + 15, "exe", "t12"))
    for i, (wid, title, exe) in enumerate(ROWS):
        y = head + i * ROW_H
        base = y + 14.5
        inactive = lambda s, wid=wid: vis(bool(wid) and s.target == wid and not s.list_focus)  # noqa: E731
        active = lambda s, wid=wid: vis(bool(wid) and s.target == wid and s.list_focus)  # noqa: E731
        if not m.never(inactive):
            # 選んだ行 (一覧に focus が無いとき。LVS_SHOWSELALWAYS で薄い地が残る)
            out.append(f'<rect x="{CX + 1}" y="{y}" width="{CW - 2}" height="{ROW_H}" fill="{SELECTED_INACTIVE}"{m.attr(inactive)}/>')
        out.append(f'<g clip-path="url(#clip-title)">{text(CX + 6, base, title, "t12")}</g>')
        out.append(f'<g clip-path="url(#clip-exe)">{text(EXE_X + 6, base, exe, "t12")}</g>')
        if not m.never(active):
            out.append(
                f"<g{m.attr(active)}>"
                f'<rect x="{CX + 1}" y="{y}" width="{CW - 2}" height="{ROW_H}" fill="{SELECTED}"/>'
                f'<g clip-path="url(#clip-title)">{text(CX + 6, base, title, "t12 wh")}</g>'
                f'<g clip-path="url(#clip-exe)">{text(EXE_X + 6, base, exe, "t12 wh")}</g></g>'
            )
    out.append("</g>")

    # プレビューの欄
    out.append(f'<rect x="{CX}" y="{PANEL_Y}" width="{CW}" height="{PANEL_H}" fill="{PANEL}"/>')
    for name, stroke, width, dash in (("empty", LINE, 1, ' stroke-dasharray="4 3"'), ("cand", ORANGE, 3, ""), ("target", BLUE, 3, "")):
        fn = lambda s, name=name: vis(role(s) == name)  # noqa: E731
        if m.never(fn):
            continue
        h = width / 2
        out.append(
            f'<rect x="{CX + h}" y="{PANEL_Y + h}" width="{CW - width}" height="{PANEL_H - width}" fill="none" '
            f'stroke="{stroke}" stroke-width="{width}"{dash}{m.attr(fn)}/>'
        )
    empty = lambda s: vis(role(s) == "empty")  # noqa: E731
    if not m.never(empty):
        out.append(text(CX + CW / 2, PANEL_Y + PANEL_H / 2 + 4, "打つ先を選ぶと、ここに映る", "t12 mu", "middle", m.attr(empty)))
    for win in WINDOWS.values():
        out.append(thumbnail(m, win))
    badge_top = PANEL_Y + PANEL_H - 6 - 20
    cand = lambda s: vis(role(s) == "cand")  # noqa: E731
    if not m.never(cand):
        out.append(
            f"<g{m.attr(cand)}>"
            f'<rect x="{CX + 6}" y="{badge_top}" width="40" height="20" rx="10" fill="{ORANGE}"/>'
            + text(CX + 26, badge_top + 14.5, "候補", "t12 b", "middle")
            + "</g>"
        )
    target = lambda s: vis(role(s) == "target")  # noqa: E731
    if not m.never(target):
        out.append(
            f"<g{m.attr(target)}>"
            f'<rect x="{CX + 6}" y="{badge_top}" width="66" height="20" rx="10" fill="{BLUE}"/>'
            + text(CX + 14, badge_top + 14.5, "打つ先", "t12 b wh")
            + f'<circle cx="{CX + 59}" cy="{badge_top + 10}" r="3.5" fill="{RED}"/></g>'
        )
    # プレビューの下の題と exe
    for win in WINDOWS.values():
        fn = lambda s, wid=win.id: vis(shown(s) == wid)  # noqa: E731
        if not m.never(fn):
            out.append(text(CX, PANEL_Y + PANEL_H + 15, f"{win.title} ({win.exe})", "t12 mu", attr=m.attr(fn)))

    # 打ち方・配列・間隔・常に手前
    y = PANEL_Y + PANEL_H + 37
    out.append(text(CX, y, "打ち方", "t13 b"))
    out.append(f"<g{busy}>")
    for i, label in enumerate(("キーの位置 (VNC・RDP 向け)", "文字 (Unicode)")):
        cy = y + 14 + i * 18
        out.append(f'<circle cx="{CX + 8}" cy="{cy}" r="6" fill="#fff" stroke="#555"/>')
        if i == 0:
            out.append(f'<circle cx="{CX + 8}" cy="{cy}" r="3" fill="#333"/>')
        out.append(text(CX + 20, cy + 4.5, label, "t13"))
    y += 46
    out.append(text(CX, y + 16, "打つ先の配列", "t13"))
    out.append(f'<rect x="760.5" y="{y + 0.5}" width="139" height="23" rx="3" fill="#fff" stroke="#a6abb3"/>')
    out.append(text(768, y + 16, "英語 (US)", "t13"))  # 既定の配列 (src/settings.rs の DEFAULT_LAYOUT)
    out.append(f'<path d="M884,{y + 10}l4,4l4,-4" fill="none" stroke="#444" stroke-width="1.3"/>')
    out.append("</g>")
    y += 29
    out.append(text(CX, y + 16, "1 文字ごとの間隔 (ms)", "t13"))
    out.append(f"<g{busy}>")
    out.append(f'<rect x="760.5" y="{y + 0.5}" width="69" height="23" fill="#fff" stroke="#a6abb3"/>')
    out.append(text(768, y + 16, "30", "t13"))
    out.append(f'<rect x="813.5" y="{y + 1.5}" width="15" height="21" fill="#f0f0f0" stroke="#d0d0d0"/>')
    out.append(f'<path d="M817.5,{y + 9}l3.5,-3.5l3.5,3.5M817.5,{y + 15}l3.5,3.5l3.5,-3.5" fill="none" stroke="#444" stroke-width="1.1"/>')
    out.append("</g>")
    y += 29
    out.append(f'<rect x="{CX + 0.5}" y="{y + 2.5}" width="13" height="13" rx="2" fill="#fff" stroke="#555"/>')
    out.append(f'<path d="M{CX + 3.5},{y + 9}l3,3l5,-6" fill="none" stroke="{INK}" stroke-width="1.8"/>')
    out.append(text(CX + 20, y + 13, "常に手前", "t13"))

    # クリップボード (文字数が変わったら、行を一瞬黄色にする)
    y += 22
    for (_, before), (t, s) in zip(m.snaps, m.snaps[1:]):
        if s.clip != before.clip:
            out.append(
                f'<rect x="{CX - 4}" y="{y}" width="160" height="18" rx="3" fill="#fde68a"'
                f'{m.pulse(t, SHOWN, HIDDEN, 1.4, "ease-in")}/>'
            )
    out.append(text(CX, y + 13, "クリップボード: 空", "t13", attr=m.attr(lambda s: vis(not s.clip))))
    if not m.never(lambda s: vis(bool(s.clip))):
        out.append(text(CX, y + 13, f"クリップボード: {TOTAL} 文字", "t13", attr=m.attr(lambda s: vis(bool(s.clip)))))

    # クリップボードを打つ・試し打ち
    out.append(f"<g{busy}>")
    out.append(button(TYPE_BTN, "クリップボードを打つ"))
    out.append(button(TRIAL_BTN, "試し打ち"))
    out.append(press(m, sc, "type") + "</g>")

    # 状態の行と進み具合
    y = TYPE_BTN[1] + TYPE_BTN[3] + 20
    statuses, kinds = [], []
    for _, s in m.snaps:
        if s.status not in statuses:
            statuses.append(s.status)
        if s.status[0] not in kinds:
            kinds.append(s.status[0])
    for st in statuses:
        label, color, _ = STATUS[st[0]]
        label = label.format(n=st[1] if len(st) > 1 else 0, total=TOTAL)
        out.append(text(CX, y, label, f"t13 b {color}".strip(), attr=m.attr(lambda s, st=st: vis(s.status == st))))
    for kind in kinds:
        out.append(text(CX, y + 16, STATUS[kind][2], "t12 mu", attr=m.attr(lambda s, kind=kind: vis(s.status[0] == kind))))
    bar_y = y + 23
    out.append(f'<rect x="{CX + 0.5}" y="{bar_y + 0.5}" width="{CW - 1}" height="7" rx="2" fill="#e6e6e6" stroke="#bcbcbc"/>')
    fill = m.attr(
        lambda s: f"transform:scaleX({num(s.progress[0] / TOTAL)});fill:{BAR_PAUSED if s.progress[1] == 'paused' else BAR}",
        cls="tl",
    )
    out.append(f'<rect x="{CX + 1}" y="{bar_y + 1}" width="{CW - 2}" height="6" rx="1.5" fill="{BAR}"{fill}/>')

    whole = m.attr(lambda s: "opacity:1;transform:translateY(0)" if s.app else "opacity:0;transform:translateY(14px)", ease=(0.35, "ease-out"))
    return f"<g{whole}>{''.join(out)}</g>"


def overlays(m: Motion, sc: Scene) -> list:
    out = []
    x, y, w, h = TOAST
    toast = m.attr(lambda s: "opacity:1;transform:translateY(0)" if s.toast else "opacity:0;transform:translateY(8px)", ease=(0.3, "ease-out"))
    if not m.never(lambda s: vis(s.toast)):
        out.append(
            f'<g transform="translate({x},{y})"><g{toast}>'
            f'<rect width="{w}" height="{h}" rx="8" fill="#fff" stroke="{LINE}" filter="url(#shadow)"/>'
            + text(14, 22.5, "パスワード管理アプリ: コピーしました", "t12")
            + "</g></g>"
        )
    if not m.never(lambda s: vis(s.keycap)):
        keycap = m.attr(lambda s: "opacity:1;transform:scale(1)" if s.keycap else "opacity:0;transform:scale(.85)", ease=(0.2, "ease-out"), cls="tb")
        kx, ky = KEYCAP
        out.append(
            f'<g transform="translate({kx},{ky})"><g{keycap}>'
            '<rect width="72" height="36" rx="6" fill="#9aa0a6"/>'
            '<rect x="1" y="1" width="70" height="30" rx="5" fill="#fff"/>'
            + text(36, 21, "Enter", "t14 b", "middle")
            + "</g></g>"
        )
    # クリックした所の波紋
    for t in sc.clicks:
        cx, cy = cursor_at(sc, t)
        ripple = m.custom(
            [
                (0.0, "opacity:0;transform:scale(.45)", STEP),
                (t, "opacity:.95;transform:scale(.45)", "ease-out"),
                (t + 0.45, "opacity:0;transform:scale(2.6)", STEP),
            ],
            cls="tb",
        )
        out.append(f'<circle cx="{num(cx)}" cy="{num(cy)}" r="9" fill="none" stroke="{BLUE}" stroke-width="2.5"{ripple}/>')
    # マウスの矢印
    pos = sc.cursor
    stops = [(0.0, f"transform:translate({num(pos[0])}px,{num(pos[1])}px)", STEP)]
    for start, end, to in sc.moves:
        stops.append((start, f"transform:translate({num(pos[0])}px,{num(pos[1])}px)", MOVE))
        stops.append((end, f"transform:translate({num(to[0])}px,{num(to[1])}px)", STEP))
        pos = to
    arrow = (
        '<path d="M0,0V18L5,13L8.5,21L11.5,19.8L8,12H15Z" fill="#fff" stroke="#000" stroke-width="1.4" stroke-linejoin="round"/>'
    )
    if sc.moves:
        out.append(f"<g{m.custom(stops)}>{arrow}</g>")
    else:
        out.append(f'<g transform="translate({num(pos[0])},{num(pos[1])})">{arrow}</g>')
    return out


def build(sc: Scene) -> str:
    m = Motion(sc)
    body = desk(m, sc)
    body.append(app(m, sc))
    body.extend(overlays(m, sc))
    clips = "".join(
        f'<clipPath id="clip-{w.id}"><rect width="{w.w}" height="{w.h}" rx="8"/></clipPath>' for w in WINDOWS.values()
    )
    css = "\n".join(
        [
            f"text{{font-family:{FONT};fill:{INK}}}",
            f".mono{{font-family:{MONO}}}",
            ".t11{font-size:11px}.t12{font-size:12px}.t13{font-size:13px}.t14{font-size:14px}.t22{font-size:22px}",
            f".b{{font-weight:700}}.mu{{fill:{MUTED}}}.wh{{fill:#fff}}.ph{{fill:#888}}.term{{fill:#c8d1dc}}",
            f".c-blue{{fill:{BLUE}}}.c-green{{fill:{GREEN}}}.c-amber{{fill:{AMBER}}}",
            ".tb{transform-box:fill-box;transform-origin:50% 50%}.tl{transform-box:fill-box;transform-origin:0 50%}",
            ".blink{animation:blink 1s step-end infinite}@keyframes blink{50%{opacity:0}}",
            *m.rules,
            # 動きを減らす設定では、場面の終わりの姿 (要素の地の姿) で止める
            "@media (prefers-reduced-motion:reduce){*{animation:none!important}}",
        ]
    )
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" '
        'role="img" aria-labelledby="title desc">\n'
        "<!-- docs/images/make_usage_svgs.py で作った。直すときは script を直して作り直す -->\n"
        f'<title id="title">{esc(sc.title)}</title>\n'
        f'<desc id="desc">{esc(sc.desc)}</desc>\n'
        f"<style>\n{css}\n</style>\n"
        "<defs>"
        '<linearGradient id="desk" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#55779f"/><stop offset="1" stop-color="#9db6d2"/></linearGradient>'
        '<linearGradient id="vm-screen" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#3b5b7a"/><stop offset="1" stop-color="#7b9cbc"/></linearGradient>'
        '<filter id="shadow" x="-20%" y="-20%" width="140%" height="150%">'
        '<feDropShadow dx="0" dy="5" stdDeviation="7" flood-color="#000" flood-opacity=".28"/></filter>'
        f'<clipPath id="canvas"><rect width="{W}" height="{H}" rx="10"/></clipPath>'
        f'<clipPath id="clip-app"><rect x="{APP_X}" y="{APP_Y}" width="{APP_W}" height="{APP_H}" rx="8"/></clipPath>'
        f'<clipPath id="clip-title"><rect x="{CX}" y="{LIST_Y}" width="{EXE_X - CX - 4}" height="{LIST_H}"/></clipPath>'
        f'<clipPath id="clip-exe"><rect x="{EXE_X}" y="{LIST_Y}" width="{CR - EXE_X - 2}" height="{LIST_H}"/></clipPath>'
        f"{clips}</defs>\n"
        f'<g clip-path="url(#canvas)">\n' + "\n".join(body) + "\n</g>\n</svg>\n"
    )


def main() -> None:
    for sc in SCENES:
        path = OUT_DIR / f"{sc.name}.svg"
        path.write_text(build(sc), encoding="utf-8")
        print(f"{path.relative_to(OUT_DIR.parent.parent)}: {path.stat().st_size} bytes")


if __name__ == "__main__":
    main()
