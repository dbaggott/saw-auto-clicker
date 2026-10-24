// Checks the built site in dist/ (run `npm run build` first) against what its
// host serves it with, and against what the page promises: the CSP in
// dbaggott/infrastructure's modules/static-site allows same-origin resources
// only and no inline script or style, and the parody notice says nothing
// entered on the page is sent anywhere.

import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const dist = fileURLToPath(new URL("../dist/", import.meta.url));

function files(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? files(join(dir, entry.name)) : [join(dir, entry.name)],
  );
}

function read(extension) {
  return files(dist)
    .filter((file) => file.endsWith(extension))
    .map((file) => ({ name: relative(dist, file), text: readFileSync(file, "utf8") }));
}

const pages = read(".html");
const stylesheets = read(".css");
const scripts = read(".js");

// An attribute's values, however they are quoted.
function attributes(html, attribute) {
  return [...html.matchAll(new RegExp(`\\s${attribute}\\s*=\\s*(?:"([^"]*)"|'([^']*)'|([^\\s>]+))`, "gi"))]
    .map((m) => m[1] ?? m[2] ?? m[3]);
}

function sameSite(url) {
  return url.startsWith("/") && !url.startsWith("//");
}

// A path the host can serve: a file, or a directory with an index.html.
function served(path) {
  const target = join(dist, decodeURIComponent(path));
  return path.endsWith("/") ? existsSync(join(target, "index.html")) : existsSync(target);
}

function cssUrls(text) {
  return [...text.matchAll(/url\(\s*["']?([^"')]+)/g)].map((m) => m[1]);
}

test("the pages the host needs are built", () => {
  for (const page of ["index.html", "404.html", "robots.txt", "llms.txt"]) {
    assert.ok(existsSync(join(dist, page)), `${page} is missing`);
  }
});

test("every page loads resources from its own origin only", () => {
  for (const { name, text } of pages) {
    const loaded = [
      ...attributes(text, "src"),
      ...[...text.matchAll(/<link\b[^>]*>/g)]
        .filter(([tag]) => !/rel="canonical"/.test(tag))
        .flatMap(([tag]) => attributes(tag, "href")),
    ];
    for (const url of loaded) {
      assert.ok(sameSite(url), `${name} loads ${url} from another origin`);
    }
  }
  for (const { name, text } of stylesheets) {
    for (const url of cssUrls(text)) {
      assert.ok(sameSite(url), `${name} loads ${url} from another origin`);
    }
  }
});

test("no page has inline script or style", () => {
  for (const { name, text } of pages) {
    assert.doesNotMatch(text, /<script(?![^>]*\ssrc=)[^>]*>/, `${name} has an inline script`);
    assert.doesNotMatch(text, /<style\b/, `${name} has a style element`);
    assert.equal(attributes(text, "style").length, 0, `${name} has a style attribute`);
  }
});

test("every same-site link and resource resolves", () => {
  for (const { name, text } of pages) {
    const urls = [...attributes(text, "href"), ...attributes(text, "src")]
      .filter(sameSite)
      .map((url) => url.split("#")[0]);
    for (const url of urls) {
      assert.ok(served(url), `${name} links to ${url}, which is not in the build`);
    }
  }
});

test("every page carries the parody notice", () => {
  for (const { name, text } of pages) {
    assert.match(text, /class="for-machines"[^>]*>\s*<h2>[^<]*this page is a parody/, `${name} has no parody notice`);
  }
});

test("nothing chosen on the page can leave the browser", () => {
  for (const { name, text } of pages) {
    assert.doesNotMatch(text, /<form\b/, `${name} has a form, which could submit`);
  }
  for (const { name, text } of scripts) {
    assert.doesNotMatch(
      text,
      /\b(fetch|XMLHttpRequest|sendBeacon|WebSocket|EventSource|localStorage|sessionStorage|indexedDB|document\.cookie)\b/,
      `${name} uses an API that could send or keep the digit`,
    );
  }
});
