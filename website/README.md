# horobets.chat website

A static introduction to the messenger. HTML and CSS only: no build step,
JavaScript, third-party fonts, analytics, or hosting credentials.
The conversation is illustrative, not a screenshot or customer testimonial.

## Preview and validate

From the repository root:

```sh
python3 scripts/check-website.py
python3 -m http.server 8080 --directory website --bind 127.0.0.1
```

Open <http://127.0.0.1:8080>. The site uses relative asset paths and works under
the repository's GitHub Pages path as well as a custom domain.

## Publish with GitHub Pages

1. Push the committed workflows and website to `main`.
2. In the repository's **Settings → Pages**, select **GitHub Actions** as the build
   and deployment source.
3. Run **Actions → Deploy website → Run workflow** on `main`.
4. To serve `horobets.chat`, verify domain ownership, set the custom domain in
   **Settings → Pages**, and configure its DNS following GitHub's instructions.
   Enable **Enforce HTTPS** once the certificate is available.

The workflow packages only `website/`, never the app source, runtime data,
environment files, or notification keys. It uses the repository's scoped
`GITHUB_TOKEN` and OIDC; no personal access token is required. Deployment is
manual. The normal CI workflow checks the page on pull requests and main pushes.

This page introduces the native apps; it is not a web chat client. GitHub Pages
cannot host the Rust API or its data directory. Deploy the messenger backend
separately and configure clients with that server's HTTPS address.

GitHub references: [custom workflow](https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages),
[custom domain](https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site).
