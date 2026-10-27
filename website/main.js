(function () {
  "use strict";

  var USER = document.body.dataset.githubUser || "sebastien988";
  var API = "https://api.github.com";
  var CACHE_KEY = "gh-cache-v1:" + USER;
  var CACHE_TTL_MS = 60 * 60 * 1000;
  var HIDDEN_REPOS = [USER]; // the profile README repo isn't a "project"

  // Shown when the GitHub API is unreachable or rate-limited (60 req/hour per IP, unauthenticated)
  // and nothing is cached yet. Refresh occasionally so it doesn't drift too far from reality.
  var FALLBACK = {
    user: {
      login: USER, name: null, bio: null, avatar_url: "https://avatars.githubusercontent.com/u/75503479?v=4",
      html_url: "https://github.com/" + USER, followers: 4, following: 8, public_repos: 15, created_at: "2020-12-04T20:41:57Z"
    },
    repos: [
      { name: "myterm", description: "A native Windows terminal emulator with tabs, built in Rust (eframe/egui + egui_term/alacritty_terminal)", language: "Rust", stargazers_count: 0, forks_count: 0, updated_at: "2026-10-27T00:00:00Z" },
      { name: "factorio-ish", description: "A Rust + macroquad factory automation game, playable natively or in-browser via WASM", language: "Rust", stargazers_count: 0, forks_count: 0, updated_at: "2026-10-20T00:00:00Z" },
      { name: "minecraft-clone-cpp", description: "Minecraft-inspired voxel game clone in C++17 / OpenGL 3.3. Chunk-based world, face-culled meshing, procedural terrain, raycast block interaction.", language: "C++", stargazers_count: 0, forks_count: 0, updated_at: "2026-10-15T00:00:00Z" },
      { name: "comic-term", description: null, language: "JavaScript", stargazers_count: 0, forks_count: 0, updated_at: "2026-10-10T00:00:00Z" },
      { name: "gridline-os", description: null, language: "Shell", stargazers_count: 0, forks_count: 0, updated_at: "2026-10-05T00:00:00Z" },
      { name: "tfsa-simulator", description: null, language: "HTML", stargazers_count: 0, forks_count: 0, updated_at: "2026-09-30T00:00:00Z" }
    ].map(function (r) { r.html_url = "https://github.com/" + USER + "/" + r.name; r.fork = false; return r; })
  };

  var LANG_COLORS = {
    Rust: "#dea584", "C++": "#f34b7d", C: "#555555", JavaScript: "#f1e05a", TypeScript: "#3178c6",
    Python: "#3572A5", HTML: "#e34c26", CSS: "#663399", Shell: "#89e051", Go: "#00ADD8", Java: "#b07219", "C#": "#178600"
  };

  var state = { repos: [], sort: "stars" };

  /* ---------- Theme ---------- */
  var root = document.documentElement;
  document.getElementById("theme-toggle").addEventListener("click", function () {
    var next = root.getAttribute("data-theme") === "dark" ? "light" : "dark";
    root.setAttribute("data-theme", next);
    localStorage.setItem("theme", next);
  });

  /* ---------- Mobile menu ---------- */
  var menuBtn = document.getElementById("menu-toggle");
  var navLinks = document.getElementById("nav-links");
  function setMenu(open) {
    navLinks.classList.toggle("open", open);
    menuBtn.setAttribute("aria-expanded", String(open));
  }
  menuBtn.addEventListener("click", function () { setMenu(!navLinks.classList.contains("open")); });
  navLinks.addEventListener("click", function (e) { if (e.target.tagName === "A") setMenu(false); });

  var nav = document.querySelector(".nav");
  window.addEventListener("scroll", function () { nav.classList.toggle("scrolled", window.scrollY > 8); }, { passive: true });

  /* ---------- Reveal on scroll ---------- */
  var revealEls = document.querySelectorAll(".reveal");
  if ("IntersectionObserver" in window) {
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (en) {
        if (en.isIntersecting) { en.target.classList.add("in"); io.unobserve(en.target); }
      });
    }, { threshold: 0.12 });
    revealEls.forEach(function (el) { io.observe(el); });
  } else {
    revealEls.forEach(function (el) { el.classList.add("in"); });
  }

  /* ---------- Typing effect ---------- */
  var typed = document.getElementById("typed");
  var commands = ["open https://github.com/" + USER, "cargo run --release", "git push origin main"];
  var reduceMotion = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (reduceMotion) {
    typed.textContent = commands[0];
  } else {
    (function typeLoop(ci, i, deleting) {
      var cmd = commands[ci];
      typed.textContent = cmd.slice(0, i);
      var delay = deleting ? 30 : 70;
      if (!deleting && i === cmd.length) { delay = 1800; deleting = true; }
      else if (deleting && i === 0) { deleting = false; ci = (ci + 1) % commands.length; delay = 400; }
      setTimeout(function () { typeLoop(ci, deleting ? i - 1 : i + 1, deleting); }, delay);
    })(0, 0, false);
  }

  document.getElementById("year").textContent = new Date().getFullYear();

  /* ---------- GitHub data ---------- */
  function readCache() {
    try { return JSON.parse(localStorage.getItem(CACHE_KEY)); } catch (e) { return null; }
  }

  function getJSON(url) {
    return fetch(url, { headers: { Accept: "application/vnd.github+json" } }).then(function (res) {
      if (!res.ok) {
        var err = new Error("GitHub API " + res.status);
        err.rateLimited = res.status === 403 || res.status === 429;
        throw err;
      }
      return res.json();
    });
  }

  function load() {
    var cached = readCache();
    if (cached && Date.now() - cached.time < CACHE_TTL_MS) {
      render(cached.data);
      return;
    }
    Promise.all([
      getJSON(API + "/users/" + USER),
      getJSON(API + "/users/" + USER + "/repos?per_page=100&sort=updated")
    ]).then(function (results) {
      var data = { user: results[0], repos: results[1] };
      try { localStorage.setItem(CACHE_KEY, JSON.stringify({ time: Date.now(), data: data })); } catch (e) { /* storage full / disabled */ }
      render(data);
    }).catch(function (err) {
      var reason = err.rateLimited ? "GitHub's API rate limit was reached" : "GitHub couldn't be reached";
      if (cached) {
        render(cached.data);
        showNotice(reason + " — showing data saved on " + new Date(cached.time).toLocaleString() + ".");
      } else {
        render(FALLBACK);
        showNotice(reason + " — showing a snapshot of my repositories.");
      }
    });
  }

  function showNotice(text) {
    var el = document.getElementById("repo-notice");
    el.textContent = "";
    el.appendChild(document.createTextNode(text + " "));
    var a = document.createElement("a");
    a.href = "https://github.com/" + USER + "?tab=repositories";
    a.target = "_blank"; a.rel = "noopener";
    a.textContent = "See everything live on GitHub →";
    el.appendChild(a);
    el.hidden = false;
  }

  function render(data) {
    var user = data.user;
    var repos = (data.repos || []).filter(function (r) { return HIDDEN_REPOS.indexOf(r.name) === -1; });
    var totalStars = (data.repos || []).reduce(function (n, r) { return n + (r.stargazers_count || 0); }, 0);

    var stats = {
      followers: user.followers, following: user.following,
      public_repos: user.public_repos, total_stars: totalStars
    };
    document.querySelectorAll("[data-stat]").forEach(function (el) {
      var v = stats[el.dataset.stat];
      if (typeof v === "number") el.textContent = v.toLocaleString();
    });

    if (user.avatar_url) document.getElementById("gh-avatar").src = user.avatar_url;
    if (user.name) document.getElementById("gh-name").textContent = user.name;
    var bio = document.getElementById("gh-bio");
    bio.textContent = user.bio || bio.dataset.fallback;
    if (user.created_at) {
      var since = new Date(user.created_at).getFullYear();
      var meta = ["On GitHub since " + since];
      if (user.location) meta.push(user.location);
      if (user.company) meta.push(user.company);
      document.getElementById("gh-meta").textContent = meta.join(" · ");
    }

    var myterm = repos.filter(function (r) { return r.name === "myterm"; })[0];
    if (myterm) document.getElementById("myterm-stars").textContent = myterm.stargazers_count;

    state.repos = repos;
    renderRepos();
  }

  function sortRepos(list) {
    return list.slice().sort(function (a, b) {
      if (state.sort === "stars") {
        var d = (b.stargazers_count || 0) - (a.stargazers_count || 0);
        if (d) return d;
      }
      return new Date(b.pushed_at || b.updated_at) - new Date(a.pushed_at || a.updated_at);
    });
  }

  function timeAgo(iso) {
    var s = (Date.now() - new Date(iso).getTime()) / 1000;
    var units = [["year", 31536000], ["month", 2592000], ["week", 604800], ["day", 86400], ["hour", 3600], ["minute", 60]];
    for (var i = 0; i < units.length; i++) {
      var n = Math.floor(s / units[i][1]);
      if (n >= 1) return n + " " + units[i][0] + (n > 1 ? "s" : "") + " ago";
    }
    return "just now";
  }

  function el(tag, cls, text) {
    var e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text != null) e.textContent = text;
    return e;
  }

  function svgIcon(id) {
    var ns = "http://www.w3.org/2000/svg";
    var svg = document.createElementNS(ns, "svg");
    svg.setAttribute("class", "icon");
    svg.setAttribute("aria-hidden", "true");
    var use = document.createElementNS(ns, "use");
    use.setAttribute("href", "#" + id);
    svg.appendChild(use);
    return svg;
  }

  function repoCard(r) {
    var a = el("a", "repo-card");
    a.href = r.html_url;
    a.target = "_blank";
    a.rel = "noopener";

    var title = el("div", "repo-title");
    title.appendChild(el("h3", null, r.name));
    if (r.fork) title.appendChild(el("span", "badge", "fork"));
    else if (r.name === "myterm") title.appendChild(el("span", "badge", "featured"));
    else title.appendChild(svgIcon("i-arrow"));
    a.appendChild(title);

    a.appendChild(el("p", "repo-desc" + (r.description ? "" : " empty"), r.description || "No description yet."));

    var meta = el("div", "repo-meta");
    if (r.language) {
      var lang = el("span");
      var dot = el("i", "lang-dot");
      dot.style.background = LANG_COLORS[r.language] || "var(--muted)";
      lang.appendChild(dot);
      lang.appendChild(document.createTextNode(r.language));
      meta.appendChild(lang);
    }
    var stars = el("span"); stars.appendChild(svgIcon("i-star")); stars.appendChild(document.createTextNode(r.stargazers_count || 0));
    var forks = el("span"); forks.appendChild(svgIcon("i-fork")); forks.appendChild(document.createTextNode(r.forks_count || 0));
    meta.appendChild(stars);
    meta.appendChild(forks);
    meta.appendChild(el("span", null, "Updated " + timeAgo(r.pushed_at || r.updated_at)));
    a.appendChild(meta);
    return a;
  }

  function renderRepos() {
    var grid = document.getElementById("repo-grid");
    grid.textContent = "";
    var list = sortRepos(state.repos);
    if (!list.length) {
      grid.appendChild(el("p", "muted", "No public repositories to show yet."));
      return;
    }
    list.forEach(function (r) { grid.appendChild(repoCard(r)); });
  }

  document.querySelectorAll(".seg").forEach(function (btn) {
    btn.addEventListener("click", function () {
      state.sort = btn.dataset.sort;
      document.querySelectorAll(".seg").forEach(function (b) { b.classList.toggle("active", b === btn); });
      renderRepos();
    });
  });

  load();
})();
