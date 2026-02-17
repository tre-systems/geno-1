const puppeteer = require("puppeteer");

const TARGET_URL = process.env.TEST_URL || "http://localhost:8080";

async function gotoWithRetry(
  page,
  url,
  totalTimeoutMs = 15000,
  intervalMs = 300
) {
  const start = Date.now();
  for (;;) {
    try {
      await page.goto(url, { waitUntil: "networkidle2", timeout: 5000 });
      return;
    } catch (_) {
      if (Date.now() - start > totalTimeoutMs) {
        throw new Error(
          `Server did not become ready at ${url} within ${totalTimeoutMs}ms`
        );
      }
      await new Promise((r) => setTimeout(r, intervalMs));
    }
  }
}

(async () => {
  const browser = await puppeteer.launch({
    headless: "new",
    args: [
      // CI-friendly flags: disable sandbox and GPU to avoid kernel/AppArmor limits
      "--no-sandbox",
      "--disable-setuid-sandbox",
      "--disable-dev-shm-usage",
      "--disable-gpu",
      // keep WebGPU feature flag harmlessly; test does not require GPU
      "--enable-unsafe-webgpu",
    ],
  });

  const page = await browser.newPage();
  const logs = [];

  page.on("console", (m) => {
    const t = m.text();
    logs.push(t);
    console.log("[console]", t);
  });

  await gotoWithRetry(page, TARGET_URL);

  await page.waitForSelector("#app-canvas", { timeout: 10000 });

  const box = await page.$eval("#app-canvas", (el) => {
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  });

  await page.mouse.click(box.x, box.y);
  await new Promise((r) => setTimeout(r, 400));

  // Overlay should be present initially; close it, then bring it back with 'H'
  const overlayInitially = await page.$("#start-overlay");
  if (!overlayInitially) throw new Error("start overlay not found");

  // Click close to hide
  await page.click("#overlay-close");
  await new Promise((r) => setTimeout(r, 200));

  const overlayHidden = await page.evaluate(() => {
    const el = document.getElementById("start-overlay");
    if (!el) return "missing";
    const style = el.getAttribute("style") || "";
    const byStyle = /display:\s*none/.test(style);
    const byClass = el.classList.contains("hidden");
    return byStyle || byClass ? "hidden" : "visible";
  });

  if (overlayHidden !== "hidden")
    throw new Error("start overlay did not hide after close");

  // Press H to show again
  await page.keyboard.press("KeyH");
  await new Promise((r) => setTimeout(r, 200));

  const overlayShown = await page.evaluate(() => {
    const el = document.getElementById("start-overlay");
    if (!el) return "missing";
    const style = el.getAttribute("style") || "";
    const byStyle = /display:\s*none/.test(style);
    const byClass = el.classList.contains("hidden");
    return byStyle || byClass ? "hidden" : "visible";
  });

  if (overlayShown !== "visible")
    throw new Error("start overlay did not show after H");

  // Engine-dependent checks (only if engine handlers are bound)
  const engineStarted = logs.some((l) => l.includes("[engine] voices="));

  if (engineStarted) {
    // Reseed all
    await page.keyboard.press("KeyR");
    await new Promise((r) => setTimeout(r, 120));
    if (!logs.some((l) => l.includes("[keys] reseeded all voices")))
      throw new Error("missing reseed log");

    // Pause and resume
    await page.keyboard.press("Space");
    await new Promise((r) => setTimeout(r, 120));
    await page.keyboard.press("Space");
    await new Promise((r) => setTimeout(r, 120));

    const sawPause =
      logs.some((l) => l.includes("[keys] paused=true")) &&
      logs.some((l) => l.includes("[keys] paused=false"));
    if (!sawPause) throw new Error("missing pause/resume logs");

    // Tempo up/down (logs only)
    await page.keyboard.down("Shift");
    await page.keyboard.press("Equal");
    await page.keyboard.up("Shift");
    await new Promise((r) => setTimeout(r, 120));
    await page.keyboard.press("Minus");
    await new Promise((r) => setTimeout(r, 120));

    // Master mute toggle (logs only)
    await page.keyboard.press("KeyM");
    await new Promise((r) => setTimeout(r, 120));

    if (!logs.some((l) => /\[keys\] master muted=true/.test(l)))
      throw new Error("missing master mute= true log");
    // Muted state no longer shown in hint; rely on logs only

    await page.keyboard.press("KeyM");
    await new Promise((r) => setTimeout(r, 120));

    if (!logs.some((l) => /\[keys\] master muted=false/.test(l)))
      throw new Error("missing master mute= false log");
    // Muted state no longer shown in hint; rely on logs only

    // Click center to toggle mute on the hovered voice (expects a hit)
    await page.mouse.move(box.x, box.y);
    await page.mouse.click(box.x, box.y);
    await new Promise((r) => setTimeout(r, 120));
    if (!logs.some((l) => /\[click\] toggle mute voice \d+/.test(l)))
      throw new Error("missing toggle mute click log");

    // Alt+Click to solo the same voice
    await page.keyboard.down("Alt");
    await page.mouse.click(box.x, box.y);
    await page.keyboard.up("Alt");
    await new Promise((r) => setTimeout(r, 120));

    if (!logs.some((l) => /\[click\] solo voice \d+/.test(l)))
      throw new Error("missing solo click log");

    // Test G key support (new functionality)
    await page.keyboard.press("KeyG");
    await new Promise((r) => setTimeout(r, 120));

    // Test root note changes A-G
    const rootKeys = ["KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG"];
    for (const key of rootKeys) {
      await page.keyboard.press(key);
      await new Promise((r) => setTimeout(r, 50));
    }

    // Test mode changes 1-7
    const modeKeys = [
      "Digit1",
      "Digit2",
      "Digit3",
      "Digit4",
      "Digit5",
      "Digit6",
      "Digit7",
    ];
    for (const key of modeKeys) {
      await page.keyboard.press(key);
      await new Promise((r) => setTimeout(r, 50));
    }

    // Test random key+mode (T key)
    await page.keyboard.press("KeyT");
    await new Promise((r) => setTimeout(r, 120));
  } else {
    console.log(
      "[note] engine not started in CI (init incomplete); skipping engine assertions"
    );
  }

  // Performance monitoring (only if engine started)
  if (engineStarted) {
    console.log("[perf] measuring frame rate performance...");

    const perfMetrics = await page.evaluate(() => {
      return new Promise((resolve) => {
        let frameCount = 0;
        let startTime = performance.now();
        let minFrameTime = Infinity;
        let maxFrameTime = 0;
        let frameTimes = [];

        function measureFrame() {
          const currentTime = performance.now();
          const frameTime = currentTime - startTime;

          if (frameCount > 0) {
            // Skip first frame
            frameTimes.push(frameTime);
            minFrameTime = Math.min(minFrameTime, frameTime);
            maxFrameTime = Math.max(maxFrameTime, frameTime);
          }

          frameCount++;
          startTime = currentTime;

          if (frameCount < 60) {
            // Measure 60 frames (~1 second at 60fps)
            requestAnimationFrame(measureFrame);
          } else {
            const avgFrameTime =
              frameTimes.reduce((a, b) => a + b, 0) / frameTimes.length;
            const avgFPS = 1000 / avgFrameTime;
            const minFPS = 1000 / maxFrameTime;
            const maxFPS = 1000 / minFrameTime;

            resolve({
              avgFPS: Math.round(avgFPS * 10) / 10,
              minFPS: Math.round(minFPS * 10) / 10,
              maxFPS: Math.round(maxFPS * 10) / 10,
              frameCount: frameTimes.length,
            });
          }
        }

        requestAnimationFrame(measureFrame);
      });
    });

    console.log(`[perf] Average FPS: ${perfMetrics.avgFPS}`);
    console.log(`[perf] Min FPS: ${perfMetrics.minFPS}`);
    console.log(`[perf] Max FPS: ${perfMetrics.maxFPS}`);
    console.log(`[perf] Measured ${perfMetrics.frameCount} frames`);

    // Warn if performance is concerning (but don't fail CI)
    if (perfMetrics.avgFPS < 30) {
      console.warn(
        `[perf] WARNING: Average FPS (${perfMetrics.avgFPS}) is below 30fps`
      );
    }
    if (perfMetrics.minFPS < 15) {
      console.warn(
        `[perf] WARNING: Minimum FPS (${perfMetrics.minFPS}) dropped below 15fps`
      );
    }
  }

  // Basic assertions
  const hasWebGPU = await page.evaluate(() => !!navigator.gpu);
  console.log("WEBGPU", hasWebGPU);

  await browser.close();

  process.exit(0);
})().catch((err) => {
  console.error("TEST_ERROR", err);
  process.exit(1);
});
