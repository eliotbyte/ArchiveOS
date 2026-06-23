from worker.asset_policy import AssetPolicy, filter_track_catalog, video_format_selector
from worker.job_input import parse_job_input
from worker.ytdlp_args import cookies_args, ytdlp_extra_args
from worker.config import Config


def test_parse_legacy_url():
    parsed = parse_job_input("https://youtube.com/watch?v=abc")
    assert parsed.url.endswith("abc")
    assert parsed.mode == "once"
    assert parsed.asset_policy.video == "best"


def test_video_format_none():
    policy = AssetPolicy(video="none")
    assert video_format_selector(policy) is None


def test_filter_subtitles_none():
    assets = [
        {"kind": "subtitle", "metadata": {"language": "en", "caption_kind": "manual"}},
    ]
    policy = AssetPolicy(subtitles="none")
    assert filter_track_catalog(assets, policy) == []


def test_filter_subtitles_preferred():
    assets = [
        {"kind": "subtitle", "metadata": {"language": "en", "caption_kind": "manual"}},
        {"kind": "subtitle", "metadata": {"language": "ja", "caption_kind": "manual"}},
    ]
    policy = AssetPolicy(subtitles="preferred", subtitle_languages=["en"])
    filtered = filter_track_catalog(assets, policy, info_language="en")
    assert len(filtered) == 1
    assert filtered[0]["metadata"]["language"] == "en"


def test_cookies_args_when_configured():
    config = Config(
        core_url="http://core",
        vault_name="v",
        vault_path="/vault",
        job_poll_secs=1,
        job_lease_secs=30,
        ytdlp_update_on_start=False,
        ytdlp_auto_update=False,
        ytdlp_update_interval_secs=3600,
        ytdlp_playlist_max_items=None,
        ytdlp_cookies_path="/vault/cookies.txt",
        ytdlp_worker_dir="/vault/workers/ytdlp",
        ytdlp_cache_dir="/vault/workers/ytdlp/cache",
        ytdlp_cookies_dir="/vault/workers/ytdlp/cookies",
    )
    assert cookies_args(config) == ["--cookies", "/vault/cookies.txt"]


def test_ytdlp_extra_args_includes_ejs_and_cookies():
    config = Config(
        core_url="http://core",
        vault_name="v",
        vault_path="/vault",
        job_poll_secs=1,
        job_lease_secs=30,
        ytdlp_update_on_start=False,
        ytdlp_auto_update=False,
        ytdlp_update_interval_secs=3600,
        ytdlp_playlist_max_items=None,
        ytdlp_cookies_path="/vault/cookies.txt",
        ytdlp_worker_dir="/vault/workers/ytdlp",
        ytdlp_cache_dir="/vault/workers/ytdlp/cache",
        ytdlp_cookies_dir="/vault/workers/ytdlp/cookies",
    )
    assert ytdlp_extra_args(config) == [
        "--remote-components",
        "ejs:github",
        "--cookies",
        "/vault/cookies.txt",
    ]
