#!/usr/bin/env python3
"""Requirements ledger: quests + professions + auction-house costs, for one character.

Reads three things that already sit on the Mac, all read-only:
  * the WoW Coach collector's SavedVariables (via tools/ledger/dump.lua -> JSON)
  * Auctionator's price database (CBOR inside a Lua string) and vendor price cache
  * QuestieDB's baked Forever data (quest objectives, turn-in NPCs, item vendors)

Prints a Markdown report and appends a one-line score to a history file, so the
next run can show what changed.

Usage:
  python3 ledger.py [--char "Corvin"] [--flavor _forever_] [--account MWEBSTER999]
                    [--json out.json] [--history ~/.wowcoach/ledger]
"""
import argparse, base64, json, os, re, struct, subprocess, sys, time, zlib

WOW = "/Applications/World of Warcraft"
HERE = os.path.dirname(os.path.abspath(__file__))
DIFF_WEIGHT = {"optimal": 1.0, "medium": 0.6, "easy": 0.25, "trivial": 0.0}
DIFF_NAME = {"optimal": "orange", "medium": "yellow", "easy": "green", "trivial": "grey"}


# ---------------------------------------------------------------- formatting
def money(c):
    if c is None:
        return "—"
    c = int(round(c))
    g, s, cu = c // 10000, (c // 100) % 100, c % 100
    if g:
        return f"{g}g {s}s"
    if s:
        return f"{s}s {cu}c"
    return f"{cu}c"


def quest_colour(qlvl, clvl):
    if qlvl is None or clvl is None:
        return "?"
    d = qlvl - clvl
    if d >= 5: return "red"
    if d >= 3: return "orange"
    if d >= -2: return "yellow"
    grey_gap = 5 if clvl < 20 else 6 + clvl // 10 - 2
    if -d > grey_gap: return "grey"
    return "green"


# ---------------------------------------------------------------- CBOR
class Cbor:
    def __init__(self, data):
        self.d, self.p = data, 0

    def _len(self, ai):
        d = self.d
        if ai < 24: return ai
        if ai == 24: v = d[self.p]; self.p += 1; return v
        if ai == 25: v = struct.unpack(">H", d[self.p:self.p + 2])[0]; self.p += 2; return v
        if ai == 26: v = struct.unpack(">I", d[self.p:self.p + 4])[0]; self.p += 4; return v
        if ai == 27: v = struct.unpack(">Q", d[self.p:self.p + 8])[0]; self.p += 8; return v
        return None  # indefinite

    def read(self):
        ib = self.d[self.p]; self.p += 1
        mt, ai = ib >> 5, ib & 31
        if mt == 7:
            if ai == 20: return False
            if ai == 21: return True
            if ai in (22, 23): return None
            if ai == 25:
                h = struct.unpack(">H", self.d[self.p:self.p + 2])[0]; self.p += 2
                return struct.unpack(">e", struct.pack(">H", h))[0]
            if ai == 26:
                v = struct.unpack(">f", self.d[self.p:self.p + 4])[0]; self.p += 4; return v
            if ai == 27:
                v = struct.unpack(">d", self.d[self.p:self.p + 8])[0]; self.p += 8; return v
            if ai == 31: return StopIteration
            return ai
        n = self._len(ai)
        if mt == 0: return n
        if mt == 1: return -1 - n
        if mt in (2, 3):
            if n is None:
                parts = []
                while self.d[self.p] != 0xFF: parts.append(self.read())
                self.p += 1
                return "".join(parts) if parts and isinstance(parts[0], str) else b"".join(parts)
            s = self.d[self.p:self.p + n]; self.p += n
            return s.decode("utf-8", "replace")
        if mt == 4:
            if n is None:
                out = []
                while self.d[self.p] != 0xFF: out.append(self.read())
                self.p += 1
                return out
            return [self.read() for _ in range(n)]
        if mt == 5:
            out = {}
            if n is None:
                while self.d[self.p] != 0xFF:
                    k = self.read(); out[k] = self.read()
                self.p += 1
                return out
            for _ in range(n):
                k = self.read(); out[k] = self.read()
            return out
        if mt == 6:
            return self.read()
        raise ValueError("bad cbor")


# ---------------------------------------------------------------- collector
def load_collector(sv_dir):
    path = os.path.join(sv_dir, "WoWCoachCollector.lua")
    out = subprocess.run(["lua", os.path.join(HERE, "dump.lua"), path, "WoWCoachCollectorDB"],
                         capture_output=True, check=True).stdout
    return json.loads(out), os.path.getmtime(path)


def pick_character(db, wanted):
    chars = db.get("characters", {})
    hits = [(k, v) for k, v in chars.items() if wanted.lower() in k.lower()]
    if not hits:
        sys.exit(f"no character matching {wanted!r}; have: {', '.join(chars)}")
    hits.sort(key=lambda kv: max(kv[1].get("capturedAt") or 0, kv[1].get("vitalsAt") or 0), reverse=True)
    return hits[0]


def bag_counts(rec):
    counts = {}
    for it in (rec.get("inventory") or {}).get("contents") or []:
        if it.get("itemID"):
            counts[it["itemID"]] = counts.get(it["itemID"], 0) + (it.get("count") or 1)
    names = {it["itemID"]: it.get("name") for it in (rec.get("inventory") or {}).get("contents") or [] if it.get("itemID")}
    return counts, names


# ---------------------------------------------------------------- auctionator
def lua_string_at(b, start):
    out = bytearray(); k = start
    esc = {0x72: 13, 0x6E: 10, 0x22: 34, 0x5C: 92, 0x74: 9}
    while True:
        c = b[k]
        if c == 0x5C:
            n = b[k + 1]
            if 0x30 <= n <= 0x39:
                m = re.match(rb"\d{1,3}", b[k + 1:k + 4]); out.append(int(m.group())); k += 1 + len(m.group()); continue
            out.append(esc.get(n, n)); k += 2; continue
        if c == 0x22: return bytes(out)
        out.append(c); k += 1


def load_auctionator(sv_dir, market_candidates):
    path = os.path.join(sv_dir, "Auctionator.lua")
    if not os.path.exists(path):
        return None
    b = open(path, "rb").read()
    db, used = None, None
    for key in market_candidates:
        i = b.find(b'["' + key.encode() + b'"] = "')
        if i >= 0:
            db = Cbor(lua_string_at(b, i + len(key) + 8)).read(); used = key; break
    vendor = {}
    vi = b.find(b"AUCTIONATOR_VENDOR_PRICE_CACHE")
    if vi >= 0:
        seg = b[vi:b.find(b"\n}", vi)]
        for m in re.finditer(rb'\["(\d+)"\] = ([\d.]+)', seg):
            vendor[int(m.group(1))] = float(m.group(2))
    days = set()
    if db:
        for v in db.values():
            if isinstance(v, dict) and isinstance(v.get("h"), dict):
                days.update(int(x) for x in v["h"])
    day0 = time.mktime((2020, 1, 1, 0, 0, 0, 0, 0, -1))
    today = int((time.time() - day0) // 86400)
    return {"db": db or {}, "market": used, "vendor": vendor,
            "latest_day": max(days) if days else None, "today": today,
            "days": sorted(days)[-5:], "mtime": os.path.getmtime(path)}


def ah_price(auc, item_id):
    if not auc: return None
    v = auc["db"].get(str(item_id))
    return v.get("m") if isinstance(v, dict) else None


def buy_price(auc, item_id):
    """Cheapest known way to buy one: vendor if the cache has it, else the AH."""
    if not auc: return None, None
    vend = auc["vendor"].get(item_id)
    ah = ah_price(auc, item_id)
    if vend is not None and (ah is None or vend <= ah): return vend, "vendor"
    if ah is not None: return ah, "AH"
    return None, None


# ---------------------------------------------------------------- QuestieDB
def load_questie(addons, flavor_toc="QuestieDB_Forever.toc"):
    path = os.path.join(addons, "QuestieDB", flavor_toc)
    if not os.path.exists(path): return None
    return path


def questie_rows(toc, entity, ids):
    """{id: {field:int -> value}} for the given ids of one entity."""
    if not toc or not ids: return {}
    want = {str(i) for i in ids}
    raw = {}
    pat = re.compile(r"^## X-" + entity + r"-(\d+)-(\w+): (.*)$")
    with open(toc, "r", encoding="utf-8", errors="replace") as f:
        for line in f:
            if not line.startswith("## X-" + entity + "-"): continue
            m = pat.match(line.rstrip("\n"))
            if m and m.group(1) in want:
                raw.setdefault(int(m.group(1)), {})[m.group(2)] = m.group(3)
    out = {}
    for qid, fields in raw.items():
        row = {}
        def dec(name):
            v = fields.get(name)
            if v is None: return None
            if re.fullmatch(r"~\d+~", v):
                v = "".join(fields.get(f"{name}-{i}", "") for i in range(1, int(v.strip("~")) + 1))
            return Cbor(base64.b64decode(v)).read()
        s = dec("S")
        if isinstance(s, dict):
            for k, v in s.items():
                if isinstance(k, int): row[k] = v
        for name in fields:
            if name.isdigit():
                try: row[int(name)] = dec(name)
                except Exception: pass
        out[qid] = row
    return out


def obj_part(obj, n):
    """Objective group n (1 creatures, 2 objects, 3 items, ...): Questie bakes
    the table as a CBOR map when groups are missing and as an array otherwise."""
    if isinstance(obj, dict):
        v = obj.get(n)
    elif isinstance(obj, list) and len(obj) >= n:
        v = obj[n - 1]
    else:
        v = None
    return v if isinstance(v, list) else []


def as_ids(x):
    if isinstance(x, list): return [i for i in x if isinstance(i, int)]
    if isinstance(x, dict): return [i for i in x.values() if isinstance(i, int)]
    return []


# ---------------------------------------------------------------- grimoires
DEMON_FAMILY = {
    "Blood Pact": "Imp", "Firebolt": "Imp", "Phase Shift": "Imp", "Fire Shield": "Imp",
    "Torment": "Voidwalker", "Sacrifice": "Voidwalker", "Consume Shadows": "Voidwalker",
    "Suffering": "Voidwalker",
    "Lash of Pain": "Succubus", "Soothing Kiss": "Succubus", "Seduction": "Succubus",
    "Lesser Invisibility": "Succubus",
    "Devour Magic": "Felhunter", "Tainted Blood": "Felhunter", "Spell Lock": "Felhunter",
    "Paranoia": "Felhunter",
}
# The level each demon becomes available (class quests).
DEMON_LEVEL = {"Imp": 1, "Voidwalker": 10, "Succubus": 20, "Felhunter": 30}


def grimoire_catalogue(toc):
    """All 'Grimoire of <ability> (Rank N)' items in QuestieDB: id, ability, rank, level, vendors."""
    if not toc: return []
    out = []
    pat = re.compile(r"^## X-Item-(\d+)-S: (.*)$")
    with open(toc, "r", encoding="utf-8", errors="replace") as f:
        for line in f:
            if not line.startswith("## X-Item-"): continue
            m = pat.match(line.rstrip("\n"))
            if not m or m.group(2).startswith("~"): continue
            try: row = Cbor(base64.b64decode(m.group(2))).read()
            except Exception: continue
            name = row.get(1) if isinstance(row, dict) else None
            if not (isinstance(name, str) and name.startswith("Grimoire of ")): continue
            mm = re.match(r"Grimoire of (.+?)(?: \(Rank (\d+)\))?$", name)
            ability, rank = mm.group(1), int(mm.group(2)) if mm.group(2) else None
            family = DEMON_FAMILY.get(ability)
            if not family: continue  # warlock tomes and quest grimoires, not demon training
            out.append({"id": int(m.group(1)), "name": name, "ability": ability, "rank": rank,
                        "family": family, "level": row.get(10) or 0})
    out.sort(key=lambda g: (g["level"], g["name"]))
    return out


def known_pet_rank(pet_spells, family, ability):
    """Highest known rank of an ability, 0 if absent, None if that family was never captured."""
    fam = (pet_spells or {}).get(family)
    if not fam: return None
    best = 0
    for sp in fam.get("spells") or []:
        if sp.get("name") == ability:
            m = re.search(r"(\d+)", sp.get("rank") or "")
            best = max(best, int(m.group(1)) if m else 1)
    return best


# ---------------------------------------------------------------- main report
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--char", default="Corvin")
    ap.add_argument("--flavor", default="_forever_")
    ap.add_argument("--account", default="MWEBSTER999")
    ap.add_argument("--json")
    ap.add_argument("--history", default=os.path.expanduser("~/.wowcoach/ledger"))
    a = ap.parse_args()

    root = os.path.join(WOW, a.flavor)
    sv = os.path.join(root, "WTF", "Account", a.account, "SavedVariables")
    db, col_mtime = load_collector(sv)
    key, rec = pick_character(db, a.char)
    clvl = rec.get("level")
    realm, faction = rec.get("realm") or "", rec.get("faction") or ""
    auc = load_auctionator(sv, [f"{realm} {faction}", realm, f"{realm} {faction}".strip()])
    toc = load_questie(os.path.join(root, "Interface", "AddOns"))
    have, bag_names = bag_counts(rec)
    L = []
    w = L.append

    # ---- header / freshness
    cap = max(rec.get("capturedAt") or 0, rec.get("vitalsAt") or 0)
    w(f"# Requirements ledger — {rec.get('name')} ({rec.get('race')} {rec.get('class')}), level {clvl}")
    w("")
    w(f"- Capture: {time.strftime('%a %d %b %H:%M', time.localtime(cap))} · {rec.get('zone')}"
      f"{' / ' + rec['subZone'] if rec.get('subZone') else ''} · {money(rec.get('moneyCopper'))}"
      f" · XP {rec.get('xp')}/{rec.get('maxXP')} · rested {rec.get('restedXP')}")
    if auc and auc["latest_day"] is not None:
        age = auc["today"] - auc["latest_day"]
        fresh = "fresh" if age <= 1 else ("aging" if age <= 3 else "STALE — rescan before trusting prices")
        w(f"- Auction data: market **{auc['market']}**, last scan {age} day(s) ago ({fresh}); "
          f"{len(auc['db'])} items; days on record {len(auc['days'])}"
          + (" (single scan: a snapshot, not a trend)" if len(auc['days']) <= 1 else ""))
    else:
        w("- Auction data: **none found** — costs below use vendor prices only")
    if not toc:
        w("- QuestieDB not found — quest objectives and turn-ins limited to the collector's own data")
    w("")

    # ---- quests
    quests = rec.get("quests") or []
    qrows = questie_rows(toc, "Quest", [q.get("questID") for q in quests if q.get("questID")])
    need_items, need_npcs = set(), set()
    for qid, row in qrows.items():
        for e in obj_part(row.get(10), 3):
            if isinstance(e, list) and e: need_items.add(e[0])
        fin = row.get(3)
        if isinstance(fin, list) and fin: need_npcs.update(as_ids(fin[0]))
        for i in as_ids(row.get(21)): need_items.add(i)
    reagent_ids = {rg.get("itemID") for b in (rec.get("recipes") or {}).values()
                   for r in b.get("recipes") or [] for rg in r.get("reagents") or []}
    irows = questie_rows(toc, "Item", need_items | reagent_ids)
    nrows = questie_rows(toc, "Npc", need_npcs)
    item_name = lambda i: (irows.get(i, {}).get(1) or bag_names.get(i) or f"item {i}")

    q_out, total_short_cost, ready, obj_have, obj_need = [], 0, 0, 0, 0
    for q in quests:
        qid, title, qlvl, state = q.get("questID"), q.get("title"), q.get("level"), q.get("state")
        row = qrows.get(qid, {})
        colour = quest_colour(qlvl, clvl)
        fin = row.get(3)
        turnin = []
        if isinstance(fin, list) and fin:
            for n in as_ids(fin[0])[:2]:
                turnin.append(nrows.get(n, {}).get(1) or f"npc {n}")
        lines, short_cost, unknown = [], 0, False
        cobj = q.get("objectives")
        qitems = {}
        for e in obj_part(row.get(10), 3):
            if isinstance(e, list) and e: qitems[item_name(e[0]).lower()] = e[0]
        if cobj:
            for o in cobj:
                h, n = o.get("have") or 0, o.get("need") or 0
                obj_have += min(h, n); obj_need += n
                label = (o.get("text") or "").split(":")[0].strip()
                extra = ""
                if o.get("type") == "item" and h < n:
                    iid = qitems.get(label.lower())
                    p, src = buy_price(auc, iid) if iid else (None, None)
                    if p is not None:
                        c = p * (n - h); short_cost += c
                        extra = f" — short {n - h}, buy ~{money(c)} ({src})"
                    else:
                        extra = f" — short {n - h}, not on the AH: farm it"
                lines.append(f"{label} {h}/{n}{' ✓' if o.get('done') else ''}{extra}")
        elif qitems:
            unknown = True
            for nm, iid in qitems.items():
                p, src = buy_price(auc, iid)
                price = f", ~{money(p)} each ({src})" if p is not None else ", not on the AH"
                lines.append(f"{item_name(iid)}: {have.get(iid, 0)} in bags{price}")
            if row.get(8):
                lines.append("<i>" + " ".join(x for x in row[8] if isinstance(x, str)) + "</i>")
        for iid in as_ids(row.get(21)):
            if not have.get(iid):
                lines.append(f"needs {item_name(iid)} in bags (not held)")
        if state == "complete": ready += 1
        total_short_cost += short_cost
        q_out.append({"id": qid, "title": title, "level": qlvl, "colour": colour, "state": state,
                      "header": q.get("header"), "turnin": turnin, "lines": lines,
                      "short_cost": short_cost, "unknown": unknown})

    order = {"complete": 0, "active": 1, "failed": 2}
    col_order = {"grey": 0, "green": 1, "yellow": 2, "orange": 3, "red": 4, "?": 5}
    q_out.sort(key=lambda r: (order.get(r["state"], 3), col_order[r["colour"]], r["level"] or 0))
    w(f"## Quests — {len(quests)} in log, **{ready} ready to hand in**")
    w("")
    w("| Lvl | Colour | Quest | Zone | State | Hand in to | Requirements |")
    w("|---|---|---|---|---|---|---|")
    for r in q_out:
        w(f"| {r['level']} | {r['colour']} | {r['title']} | {r['header'] or ''} | {r['state']} | "
          f"{', '.join(r['turnin'])} | {'<br>'.join(r['lines']) or '—'} |")
    w("")

    # ---- professions
    w("## Professions — cheapest skill-ups")
    w("")
    prof_rows, prof_summary = [], []
    recipes = rec.get("recipes") or {}
    for p in rec.get("professions") or []:
        name, rank, mx = p.get("name"), p.get("rank"), p.get("maxRank")
        prof_summary.append({"name": name, "rank": rank, "max": mx})
        book = recipes.get(name) or {}
        cands = []
        for r in book.get("recipes") or []:
            wgt = DIFF_WEIGHT.get(r.get("difficulty"), 0)
            if wgt <= 0: continue
            mcost, cash, missing, craftable = 0, 0, [], 10 ** 9
            priced = True
            for rg in r.get("reagents") or []:
                iid, cnt = rg.get("itemID"), rg.get("count") or 1
                pr, src = buy_price(auc, iid)
                if pr is None: priced = False; pr = 0
                mcost += pr * cnt
                held = have.get(iid, 0)
                craftable = min(craftable, held // cnt)
                if held < cnt:
                    cash += pr * (cnt - held)
                    missing.append(f"{cnt - held}× {item_name(iid)}")
            out_val = ah_price(auc, r.get("makesItemID")) if r.get("makesItemID") else None
            margin = (out_val * 0.95 - mcost) if out_val is not None else None
            per_point = (mcost / wgt) if priced else None
            cands.append({"recipe": r.get("name"), "colour": DIFF_NAME.get(r.get("difficulty")),
                          "mats_value": mcost if priced else None, "cash_now": cash,
                          "can_make_now": 0 if craftable == 10 ** 9 else craftable,
                          "sells_for": out_val, "margin": margin, "cost_per_point": per_point,
                          "missing": missing})
        cands.sort(key=lambda c: (c["cost_per_point"] is None, c["cost_per_point"] or 0))
        age = book.get("capturedAt")
        stale = f" (recipe list from {time.strftime('%d %b', time.localtime(age))})" if age else " (no recipe list — open the window once)"
        w(f"### {name} {rank}/{mx}{stale}")
        if not cands:
            w("No orange/yellow/green recipes known — visit the trainer.")
            w(""); continue
        w("| Recipe | Colour | Can make now | Mats value | Cost/point | Sells for (AH) | Margin after cut | Missing |")
        w("|---|---|---|---|---|---|---|---|")
        for c in cands[:8]:
            w(f"| {c['recipe']} | {c['colour']} | {c['can_make_now']} | {money(c['mats_value'])} | "
              f"{money(c['cost_per_point'])} | {money(c['sells_for'])} | "
              f"{(money(c['margin']) if c['margin'] is not None and c['margin'] >= 0 else ('-' + money(-c['margin']) if c['margin'] is not None else '—'))} | "
              f"{', '.join(c['missing']) or '—'} |")
        w("")
        prof_rows.append({"profession": name, "rank": rank, "max": mx, "best": cands[:3]})

    # ---- grimoires (warlocks)
    grim_due = []
    if (rec.get("class") or "").upper() == "WARLOCK":
        cat = grimoire_catalogue(toc)
        ps = rec.get("petSpells") or {}
        w("## Grimoires")
        w("")
        if not ps:
            w("_No demon spellbook captured yet. Summon each demon you have (Imp, Voidwalker, ...) "
              "once and `/reload` — the collector records what each one knows._")
            w("")
        due, soon, unknown_fams = [], [], set()
        for g in cat:
            if DEMON_LEVEL.get(g["family"], 99) > (clvl or 0): continue
            have_rank = known_pet_rank(ps, g["family"], g["ability"])
            need_rank = g["rank"] or 1
            if have_rank is None:
                unknown_fams.add(g["family"])
                status = "unknown"
            elif have_rank >= need_rank:
                continue
            else:
                status = "not learned"
            price, src = buy_price(auc, g["id"])
            entry = dict(g, status=status, price=price)
            if g["level"] <= (clvl or 0): due.append(entry)
            elif g["level"] <= (clvl or 0) + 4: soon.append(entry)
        if due:
            w("**Available now and not learned — buy from your demon trainer:**")
            w("")
            w("| Grimoire | Demon | Level | Status | Price |")
            w("|---|---|---|---|---|")
            for g in due:
                w(f"| {g['name']} | {g['family']} | {g['level']} | {g['status']} | {money(g['price']) if g['price'] else 'trainer price'} |")
            w("")
        else:
            w("Every grimoire available at your level is learned.")
            w("")
        if soon:
            w("Coming up: " + "; ".join(f"{g['name']} at {g['level']}" for g in soon))
            w("")
        if unknown_fams:
            w(f"_Status 'unknown': no spellbook captured yet for {', '.join(sorted(unknown_fams))} — "
              "summon it once and `/reload`._")
            w("")
        grim_due = [g["name"] for g in due if g["status"] == "not learned"]

    # ---- bag value
    w("## Bags — what is worth selling")
    w("")
    used = set(need_items)
    for r in (rec.get("recipes") or {}).values():
        for rr in r.get("recipes") or []:
            for rg in rr.get("reagents") or []: used.add(rg.get("itemID"))
    sell = []
    for iid, cnt in have.items():
        if iid in used: continue
        p = ah_price(auc, iid)
        if p: sell.append((p * cnt, iid, cnt, p))
    sell.sort(reverse=True)
    w("| Item | Qty | AH each | AH total |")
    w("|---|---|---|---|")
    for tot, iid, cnt, p in sell[:12]:
        w(f"| {bag_names.get(iid) or iid} | {cnt} | {money(p)} | {money(tot)} |")
    w("")
    w("_Excludes anything a quest in the log or a known recipe uses. AH prices are before the 5% cut; "
      "cheap stacks are often better vendored than listed._")
    w("")

    # ---- score + history
    score = {
        "t": int(time.time()), "capture": cap, "level": clvl, "xp": rec.get("xp"), "maxXP": rec.get("maxXP"),
        "money": rec.get("moneyCopper"), "quests": len(quests), "ready": ready,
        "objectives_have": obj_have, "objectives_need": obj_need,
        "shortfall_cost": int(total_short_cost),
        "professions": {p["name"]: p["rank"] for p in prof_summary},
        "grimoires_due": grim_due,
    }
    hist_dir = a.history
    os.makedirs(hist_dir, exist_ok=True)
    hist = os.path.join(hist_dir, re.sub(r"[^A-Za-z0-9]+", "_", key) + ".jsonl")
    prev = None
    if os.path.exists(hist):
        with open(hist) as f:
            lines = [l for l in f if l.strip()]
        if lines:
            prev = json.loads(lines[-1])
    if not prev or prev.get("capture") != cap:
        with open(hist, "a") as f: f.write(json.dumps(score) + "\n")
    w("## Score")
    w("")
    pct = f"{100 * obj_have // obj_need}%" if obj_need else "n/a (objective counts not captured yet)"
    w(f"- Quest objectives done: **{pct}** · ready to hand in: **{ready}** · cost to buy every buyable shortfall: **{money(total_short_cost)}**")
    w(f"- Professions: " + ", ".join(f"{p['name']} {p['rank']}/{p['max']}" for p in prof_summary))
    if prev and prev.get("capture") != cap:
        dl = []
        if prev.get("level") != clvl: dl.append(f"level {prev.get('level')} → {clvl}")
        dm = (rec.get("moneyCopper") or 0) - (prev.get("money") or 0)
        dl.append(f"money {'+' if dm >= 0 else '-'}{money(abs(dm))}")
        for n, r in score["professions"].items():
            o = (prev.get("professions") or {}).get(n)
            if o is not None and o != r: dl.append(f"{n} {o} → {r}")
        dq = ready - (prev.get("ready") or 0)
        dl.append(f"ready-to-hand-in {prev.get('ready')} → {ready}")
        w(f"- Since last ledger ({time.strftime('%d %b %H:%M', time.localtime(prev.get('capture') or 0))}): " + "; ".join(dl))
    w("")
    print("\n".join(L))
    if a.json:
        with open(a.json, "w") as f:
            json.dump({"character": key, "score": score, "quests": q_out, "professions": prof_rows}, f, indent=1)


if __name__ == "__main__":
    main()
