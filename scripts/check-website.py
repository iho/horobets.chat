#!/usr/bin/env python3
"""Check the dependency-free website before previewing or publishing it."""

from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit
import re
import sys

ROOT = Path(__file__).resolve().parent.parent / "website"


class Page(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.ids = set()
        self.links = []
        self.errors = []
        self.lang = None
        self.has_viewport = False
        self.has_description = False
        self.has_title = False

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "html":
            self.lang = attrs.get("lang")
        if tag == "title":
            self.has_title = True
        if tag == "meta":
            self.has_viewport |= attrs.get("name") == "viewport"
            self.has_description |= (
                attrs.get("name") == "description" and bool(attrs.get("content"))
            )
        if "id" in attrs:
            if attrs["id"] in self.ids:
                self.errors.append(f"Duplicate id: {attrs['id']}")
            self.ids.add(attrs["id"])
        for name in ("href", "src"):
            if name in attrs:
                self.links.append(attrs[name])
        if tag == "img" and "alt" not in attrs:
            self.errors.append("Image is missing alt text")


def check():
    page = Page()
    page.feed((ROOT / "index.html").read_text())
    errors = page.errors
    if not all((page.lang, page.has_title, page.has_viewport, page.has_description)):
        errors.append("Missing language, title, viewport, or description")
    for link in page.links:
        url = urlsplit(link)
        if url.scheme or url.netloc:
            if url.scheme != "https" or not url.netloc:
                errors.append(f"Expected an HTTPS link: {link}")
            continue
        if not url.path and url.fragment and unquote(url.fragment) not in page.ids:
            errors.append(f"Missing anchor: {link}")
        if url.path:
            target = (ROOT / unquote(url.path).lstrip("/")).resolve()
            if not target.is_relative_to(ROOT) or not target.is_file():
                errors.append(f"Missing or out-of-site asset: {link}")
    for css in ROOT.glob("*.css"):
        for asset in re.findall(r"url\(\s*['\"]?([^)'\"\s]+)", css.read_text()):
            target = (css.parent / unquote(asset)).resolve()
            if not target.is_relative_to(ROOT) or not target.is_file():
                errors.append(f"Missing CSS asset in {css.name}: {asset}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"Website checks passed ({len(page.links)} links and assets).")
    return 0


if __name__ == "__main__":
    sys.exit(check())
