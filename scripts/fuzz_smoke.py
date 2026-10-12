#!/usr/bin/env python3
"""Run short, resource-bounded cargo-fuzz campaigns from the checked-in corpora."""

import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGETS = {
    "frontend": 16 * 1024,
    "lsp_frames": 64 * 1024,
}


def main() -> None:
    for target, max_len in TARGETS.items():
        corpus = ROOT / "fuzz" / "corpus" / target
        artifacts = ROOT / "fuzz" / "artifacts" / target
        artifacts.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix=f"nacre-fuzz-{target}-") as directory:
            campaign_corpus = Path(directory) / target
            shutil.copytree(corpus, campaign_corpus)
            command = [
                "cargo",
                "+nightly",
                "fuzz",
                "run",
                target,
                str(campaign_corpus),
                "--",
                "-max_total_time=30",
                "-timeout=5",
                "-rss_limit_mb=512",
                f"-max_len={max_len}",
                "-print_final_stats=1",
            ]
            subprocess.run(command, cwd=ROOT, check=True, timeout=300)


if __name__ == "__main__":
    main()


