#!/usr/bin/env python3
"""Download the Chinese Phoebe voice lines exposed by the Kurobbs wiki entry."""

from __future__ import annotations

import csv
import json
import re
import ssl
import time
import uuid
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen


ENTRY_ID = "1309523456688947200"
ENTRY_URL = f"https://wiki.kurobbs.com/mc/item/{ENTRY_ID}"
API_URL = "https://api.kurobbs.com/wiki/core/catalogue/item/getEntryDetail"
AUDIO_PREFIX = "https://web-static.kurobbs.com/wiki_audio/"
OUTPUT_DIR = Path(__file__).resolve().parent
WAV_DIR = OUTPUT_DIR / "wav"
USER_AGENT = "Mozilla/5.0 (compatible; personal-dataset-downloader/1.0)"

try:
    import certifi

    SSL_CONTEXT = ssl.create_default_context(cafile=certifi.where())
except ImportError:
    SSL_CONTEXT = ssl.create_default_context()


def fetch_json() -> dict:
    query = urlencode({"_t": int(time.time() * 1000)})
    body = urlencode({"id": ENTRY_ID}).encode("utf-8")
    request = Request(
        f"{API_URL}?{query}",
        data=body,
        method="POST",
        headers={
            "Accept": "application/json, text/plain, */*",
            "Content-Type": "application/x-www-form-urlencoded;charset=UTF-8",
            "Origin": "https://wiki.kurobbs.com",
            "Referer": ENTRY_URL,
            "source": "h5",
            "wiki_type": "9",
            "devcode": uuid.uuid4().hex,
            "User-Agent": USER_AGENT,
        },
    )
    with urlopen(request, timeout=30, context=SSL_CONTEXT) as response:
        payload = json.load(response)
    if payload.get("code") != 200:
        raise RuntimeError(f"Wiki API returned: {payload}")
    return payload


def extract_chinese_audio(payload: dict) -> list[dict]:
    found: list[dict] = []

    def walk(node: object, in_chinese_tab: bool = False) -> None:
        if isinstance(node, dict):
            title = str(node.get("title", ""))
            chinese = in_chinese_tab or title.startswith("中文")
            media_list = node.get("mediaList")
            if chinese and isinstance(media_list, list):
                for item in media_list:
                    if not isinstance(item, dict):
                        continue
                    url = str(item.get("playUrl", ""))
                    if url.startswith(AUDIO_PREFIX) and url.lower().endswith(".wav"):
                        found.append(
                            {
                                "id": str(item.get("id", "")),
                                "title": str(item.get("audioTitle", "untitled")),
                                "text": normalize_text(str(item.get("content", ""))),
                                "url": url,
                                "file_size": int(item.get("fileSize") or 0),
                            }
                        )
            for value in node.values():
                walk(value, chinese)
        elif isinstance(node, list):
            for value in node:
                walk(value, in_chinese_tab)

    walk(payload)
    unique: dict[str, dict] = {}
    for item in found:
        unique.setdefault(item["url"], item)
    return list(unique.values())


def normalize_text(text: str) -> str:
    return re.sub(r"\s+", " ", text).replace("|", "｜").strip()


def safe_title(title: str) -> str:
    cleaned = re.sub(r'[\\/:*?"<>|\s]+', "_", title).strip("._")
    return cleaned or "untitled"


def download(url: str, destination: Path, expected_size: int) -> None:
    if destination.exists():
        size_ok = expected_size <= 0 or destination.stat().st_size == expected_size
        if size_ok and destination.read_bytes()[:4] == b"RIFF":
            return

    request = Request(url, headers={"Referer": ENTRY_URL, "User-Agent": USER_AGENT})
    last_error: Exception | None = None
    for attempt in range(3):
        try:
            with urlopen(request, timeout=45, context=SSL_CONTEXT) as response:
                data = response.read()
            if data[:4] != b"RIFF":
                raise RuntimeError(f"Not a RIFF/WAV response: {url}")
            if expected_size > 0 and len(data) != expected_size:
                raise RuntimeError(
                    f"Size mismatch for {url}: expected {expected_size}, got {len(data)}"
                )
            destination.write_bytes(data)
            return
        except (HTTPError, URLError, TimeoutError, RuntimeError) as error:
            last_error = error
            if attempt < 2:
                time.sleep(1.5 * (attempt + 1))
    raise RuntimeError(f"Failed to download {url}: {last_error}")


def write_metadata(items: list[dict]) -> None:
    manifest_path = OUTPUT_DIR / "metadata.json"
    tsv_path = OUTPUT_DIR / "metadata.tsv"
    list_path = OUTPUT_DIR / "phoebe_zh.list"

    manifest_path.write_text(
        json.dumps(
            {
                "source_page": ENTRY_URL,
                "entry_id": ENTRY_ID,
                "language": "zh",
                "speaker": "菲比",
                "voice_actor": "傅婷云",
                "count": len(items),
                "items": items,
            },
            ensure_ascii=False,
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )

    with tsv_path.open("w", encoding="utf-8", newline="") as file:
        writer = csv.writer(file, delimiter="\t")
        writer.writerow(["filename", "title", "text", "source_url", "file_size"])
        for item in items:
            writer.writerow(
                [
                    item["filename"],
                    item["title"],
                    item["text"],
                    item["url"],
                    item["file_size"],
                ]
            )

    with list_path.open("w", encoding="utf-8") as file:
        for item in items:
            wav_path = (WAV_DIR / item["filename"]).resolve()
            file.write(f"{wav_path}|菲比|zh|{item['text']}\n")


def main() -> None:
    WAV_DIR.mkdir(parents=True, exist_ok=True)
    items = extract_chinese_audio(fetch_json())
    if not items:
        raise RuntimeError("No Chinese WAV entries were found in the wiki response")

    print(f"Found {len(items)} unique Chinese voice lines")
    for index, item in enumerate(items, start=1):
        filename = f"{index:03d}_{safe_title(item['title'])}.wav"
        item["filename"] = filename
        print(f"[{index:02d}/{len(items)}] {filename}")
        download(item["url"], WAV_DIR / filename, item["file_size"])
        time.sleep(0.1)

    write_metadata(items)
    total_bytes = sum((WAV_DIR / item["filename"]).stat().st_size for item in items)
    print(f"Done: {len(items)} WAV files, {total_bytes / 1024 / 1024:.2f} MiB")


if __name__ == "__main__":
    main()
