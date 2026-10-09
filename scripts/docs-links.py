"""Keep GitHub Markdown links usable when building the documentation website."""
from pathlib import Path
import re
from urllib.parse import quote, unquote, urlsplit

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"
LINK = re.compile(r"(?<!!)\[([^\]]+)\]\(([^\s)]+)\)")


def on_page_markdown(markdown, page, config, files):
    def replace(match):
        label, target = match.groups()
        url = urlsplit(target)
        if url.scheme or url.netloc or not url.path:
            return match.group(0)
        resolved = (DOCS / page.file.src_uri).parent.joinpath(unquote(url.path)).resolve()
        if resolved.is_relative_to(DOCS):
            return match.group(0)
        if not resolved.is_relative_to(ROOT) or not resolved.exists():
            raise ValueError(f"{page.file.src_uri}: missing repository link {target}")
        relative = resolved.relative_to(ROOT).as_posix()
        kind = "tree" if resolved.is_dir() else "blob"
        repository = config["repo_url"].rstrip("/")
        suffix = ("?" + url.query if url.query else "") + ("#" + url.fragment if url.fragment else "")
        return f"[{label}]({repository}/{kind}/main/{quote(relative)}{suffix})"

    return LINK.sub(replace, markdown)
