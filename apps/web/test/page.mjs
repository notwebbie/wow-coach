// Drives apps/web/index.html in headless Chromium twice: once with no pkg/
// (the bug the user hit) and once with a JS stand-in for the engine.
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { dirname, extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const WEB = join(HERE, "..");
const STUB = join(HERE, "stub");

let serveStub = false;

const server = createServer(async (req, res) => {
  const path = normalize(decodeURIComponent(req.url.split("?")[0]));
  const rel = path === "/" ? "/index.html" : path;
  const file = rel.startsWith("/pkg/")
    ? (serveStub ? join(STUB, rel.slice(5)) : null)
    : join(WEB, rel);
  if (file === null) { res.writeHead(404).end("no build"); return; }
  try {
    const body = await readFile(file);
    const type = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm" }[extname(file)] || "text/plain";
    res.writeHead(200, { "content-type": type }).end(body);
  } catch { res.writeHead(404).end("missing"); }
});
await new Promise(r => server.listen(8099, r));

// CI installs its own Chromium; a sandbox that ships one points at it.
const browser = await chromium.launch(
  process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}
);
const results = [];
function check(name, ok, detail = "") { results.push({ name, ok, detail }); }

// --- Case A: no pkg/. This is exactly what the user saw. ---
{
  serveStub = false;
  const page = await browser.newPage();
  await page.goto("http://127.0.0.1:8099/");
  await page.waitForTimeout(500);

  const errorVisible = await page.isVisible("#error");
  check("A: missing build is reported on screen", errorVisible,
        errorVisible ? await page.textContent("#error") : "error box still hidden");

  // The listener must be attached even though the engine failed.
  let opened = false;
  page.on("filechooser", async c => { opened = true; await c.setFiles([]); });
  await page.click("#pick");
  await page.waitForTimeout(300);
  check("A: the choose-the-file button is alive", opened,
        opened ? "" : "clicking #pick did nothing — listeners were never attached");
}

// --- Case B: engine present. The whole flow should work. ---
{
  serveStub = true;
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", e => errors.push(String(e)));
  await page.goto("http://127.0.0.1:8099/");
  await page.waitForTimeout(500);

  check("B: no error box on a good load", !(await page.isVisible("#error")));

  await page.setInputFiles("#file", {
    name: "WoWCoachCollector.lua",
    mimeType: "text/plain",
    buffer: Buffer.from('WoWCoachCollectorDB = { characters = {} }\n')
  });
  await page.waitForTimeout(500);

  const rosterShown = await page.isVisible("#roster-section");
  check("B: the roster appears after choosing a file", rosterShown);

  if (rosterShown) {
    await page.click("#all");
    await page.waitForTimeout(400);
    const answer = await page.textContent("#answer-section").catch(() => "");
    check("B: a recommendation is rendered", answer.includes("Gravehexx"), answer.trim().slice(0, 140));
  }
  // --- The detail screens -------------------------------------------
  const planText = await page.textContent("#plan-section").catch(() => "");
  check("B: the plan section defaults to the recommended character",
        (await page.inputValue("#plan-who")) === "7:forever|16:Classic Beta PvE|9:Gravehexx",
        await page.inputValue("#plan-who"));

  check("B: quests name the zone worth going to", planText.includes("Head to Westfall"),
        planText.replace(/\s+/g, " ").slice(0, 120));
  check("B: a finished quest is surfaced as a hand-in", planText.includes("Hand in Cutting Teeth"));
  check("B: a tight log is called out", planText.includes("Log nearly full"));

  // The distinction this project keeps getting wrong.
  check("B: a profession with no captured recipes says so",
        planText.includes("No recipe list captured"),
        "Enchanting must not read as 'nothing to make'");
  check("B: a profession whose recipes are all grey says something different",
        planText.includes("Every recipe you know here is grey"));
  check("B: the two are not the same sentence",
        planText.includes("No recipe list captured") && planText.includes("Every recipe you know here is grey"));

  check("B: training advice appears when a trainer can help",
        planText.includes("A trainer will take it to 150"));
  check("B: a missing reagent is named with its shortfall", planText.includes("need 4 Wool Cloth"));
  check("B: banked materials are flagged", planText.includes("(bank)"));

  // Another character's professions must not leak in.
  check("B: only the chosen character's professions are shown",
        planText.includes("Tailoring") && !planText.includes("Mining"),
        planText.includes("Mining") ? "Vardril's Mining leaked into Gravehexx's plan" : "");

  // --- Switching character ------------------------------------------
  await page.selectOption("#plan-who", "7:forever|16:Classic Beta PvE|7:Vardril");
  await page.waitForTimeout(300);
  const second = await page.textContent("#plan-section").catch(() => "");

  check("B: switching character redraws the plan",
        second.includes("Mining") && !second.includes("Tailoring"),
        second.replace(/\s+/g, " ").slice(0, 120));
  // Asserted on the marker rather than the sentence: a wording change must not
  // be able to turn this check green while the page still offers a zone that
  // has nothing in it worth doing.
  const bestMarked = await page.$(".zone.best");
  check("B: a roster with nowhere worth going does not invent a destination",
        bestMarked === null,
        bestMarked === null ? "" : "a zone was marked 'go here' despite worthATrip 0");
  check("B: and says so in words", second.includes("Nothing in the log can be cleared"));
  check("B: the level gate is distinguished from the skill gate",
        second.includes("needs level 10"));

  // --- Quest links ---------------------------------------------------
  await page.selectOption("#plan-who", "7:forever|16:Classic Beta PvE|9:Gravehexx");
  await page.waitForTimeout(300);

  const links = await page.$$eval("#plan-quests a", nodes =>
    nodes.map(a => ({ href: a.getAttribute("href"), rel: a.getAttribute("rel"), text: a.textContent })));

  check("B: quests link to the right database for the client",
        links.length > 0 && links.every(l => l.href.startsWith("https://www.wowhead.com/forever/quest=")),
        links.map(l => l.href).join(" "));
  check("B: the linked title is the quest name",
        links.some(l => l.text === "Goretusk Liver Pie"),
        links.map(l => l.text).join(" | "));
  check("B: links do not leak the page address",
        links.every(l => (l.rel || "").includes("noreferrer")));
  check("B: no link is built from a missing id",
        !links.some(l => /quest=(null|undefined|NaN)/.test(l.href)),
        links.map(l => l.href).join(" "));

  // A quest with no captured id must still appear — as text, not a dead link.
  const dropText = await page.textContent(".housekeeping").catch(() => "");
  check("B: a quest with no id is still shown, unlinked",
        dropText.includes("Kobold Candles") && !links.some(l => l.text === "Kobold Candles"),
        dropText.replace(/\s+/g, " ").slice(0, 90));

  check("B: the zone worth going to is marked, not just ordered",
        ((await page.getAttribute(".zone", "class")) || "").includes("best"),
        await page.getAttribute(".zone", "class"));

  // --- Gear ------------------------------------------------------------
  const gearText = await page.textContent("#plan-gear").catch(() => "");
  check("B: a missing step-change item is surfaced with its reason",
        gearText.includes("A wand") && gearText.includes("Enchanting craft"),
        gearText.replace(/\s+/g, " ").slice(0, 110));
  check("B: empty slots are listed", gearText.includes("Shoulder") && gearText.includes("Legs"));
  check("B: an unverified claim says so on screen",
        gearText.includes("not checked against a Forever source"),
        "a reader must be able to weigh the advice");
  check("B: an undecidable milestone is admitted, not guessed",
        gearText.includes("Cannot tell whether a ranged weapon is equipped"),
        gearText.replace(/\s+/g, " ").slice(0, 110));

  // The absent-vs-empty case, on the character with no capture.
  await page.selectOption("#plan-who", "7:forever|16:Classic Beta PvE|7:Vardril");
  await page.waitForTimeout(300);
  const noGear = await page.textContent("#plan-gear").catch(() => "");
  check("B: a character with no equipment captured says so",
        noGear.includes("No equipment has been captured"),
        noGear.replace(/\s+/g, " ").slice(0, 100));
  check("B: and is not shown as having nothing missing",
        !noGear.includes("Nothing obvious missing"),
        "silence must not be rendered as a clean bill of health");
  await page.selectOption("#plan-who", "7:forever|16:Classic Beta PvE|9:Gravehexx");
  await page.waitForTimeout(300);

  check("B: no uncaught page errors", errors.length === 0, errors.join(" | "));
}

await browser.close();
server.close();

let failed = 0;
for (const r of results) {
  if (!r.ok) failed++;
  console.log(`${r.ok ? "pass" : "FAIL"}  ${r.name}${r.detail ? "\n        " + r.detail.replace(/\s+/g, " ").slice(0, 160) : ""}`);
}
console.log(failed ? `\n${failed} failed` : "\nall passed");
process.exit(failed ? 1 : 0);
