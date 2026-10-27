# Portfolio website

A static personal site (plain HTML, CSS and JS, no build step) that showcases my
projects and promotes my GitHub profile. The profile card, stats and repo grid
are loaded live from the public GitHub API.

```
website/
├── index.html          # page content (look for ✏️ EDIT comments)
├── styles.css          # theme + layout (dark/light)
├── main.js             # GitHub API loading, caching, fallback, UI behaviour
└── assets/myterm-icon.svg
```

## Preview locally

```
cd website
python3 -m http.server 8000
# open http://localhost:8000
```

## Turn on GitHub Pages

1. Merge this branch into `master`.
2. On GitHub, go to **Settings → Pages** for this repository.
3. Under **Build and deployment → Source**, choose **GitHub Actions**.
4. Go to **Actions → Deploy website to GitHub Pages → Run workflow** (or push
   any change under `website/`). The workflow in
   `.github/workflows/pages.yml` uploads the `website/` folder and deploys it.
5. The site will be live at `https://sebastien988.github.io/myterm/`. The URL
   is also shown in the workflow run and on the Settings → Pages screen.

Optional: to serve it from `https://sebastien988.github.io/` instead, copy the
`website/` contents into a repo named `sebastien988.github.io`.

## Customize

- **Placeholder text** is marked with `✏️ EDIT` comments in `index.html` and
  shown on the page with a dashed outline and a "Placeholder" tag. Once you've
  written your own text, remove the `placeholder` class and the
  `<span class="ph-tag">` element.
- **Bio on the GitHub card** comes from your GitHub profile. Set one at
  <https://github.com/settings/profile>; until then the `data-fallback` text on
  `#gh-bio` is used.
- **Email**: replace `you@example.com` in the Contact section (or delete that button).
- **GitHub username**: change `data-github-user` on `<body>` and the
  `github.com/sebastien988` links in `index.html`.
- **Hidden repos**: edit `HIDDEN_REPOS` in `main.js`.
- **Colours**: tweak `--accent` / `--accent-2` at the top of `styles.css`.

## How the GitHub data works

- Requests `https://api.github.com/users/<user>` and `/users/<user>/repos`
  directly from the browser. No token is used or needed.
- Responses are cached in `localStorage` for an hour, which keeps visitors well
  under the unauthenticated limit of 60 requests per hour.
- If the API fails or is rate-limited, the site shows the last cached data, or
  else a built-in snapshot (`FALLBACK` in `main.js`), plus a notice linking to
  the live GitHub profile.
