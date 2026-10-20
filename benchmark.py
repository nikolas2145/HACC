"""Benchmark HACC GPU/CPU against ZIP and RAR, verifying every extraction."""

import csv
import hashlib
import shutil
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parent
WORK = (ROOT / "benchmark_work").resolve()
RESULTS = ROOT / "benchmark_results.csv"
HACC = ROOT / "target/release/hacc.exe"
SEVEN_ZIP = Path(r"C:\Program Files\7-Zip\7z.exe")
RAR = Path(r"C:\Program Files\WinRAR\Rar.exe")


def verified_remove(path: Path) -> None:
    target = path.resolve()
    if not target.is_relative_to(WORK) or target == WORK:
        raise ValueError(f"Refusing to remove {target}")
    if target.is_dir():
        shutil.rmtree(target)
    elif target.exists():
        target.unlink()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(8 * 1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def timed(command: list[str]) -> float:
    start = time.perf_counter()
    process = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, errors="replace")
    duration = time.perf_counter() - start
    if process.returncode:
        raise RuntimeError(f"Command failed ({process.returncode}): {command}\n{process.stdout}\n{process.stderr}")
    return duration


def benchmark(source: Path, method: str, round_number: int) -> dict:
    archive = WORK / f"{source.stem}_{method}_{round_number}.{ 'hacc' if method.startswith('hacc') else method }"
    output = WORK / f"output_{source.stem}_{method}_{round_number}"
    output.mkdir(parents=True)
    if method == "hacc_gpu":
        compress = [str(HACC), "a", "-g", str(archive), source.name]
        extract = [str(HACC), "x", str(archive), str(output)]
    elif method == "hacc_cpu":
        compress = [str(HACC), "a", "-l3", str(archive), source.name]
        extract = [str(HACC), "x", str(archive), str(output)]
    elif method == "zip":
        compress = [str(SEVEN_ZIP), "a", "-tzip", "-mm=Deflate", "-mx=5", "-bd", "-bso0", "-bsp0", str(archive), source.name]
        extract = [str(SEVEN_ZIP), "x", "-y", "-bd", "-bso0", "-bsp0", f"-o{output}", str(archive)]
    elif method == "rar":
        compress = [str(RAR), "a", "-idq", "-ep", "-m3", str(archive), source.name]
        extract = [str(RAR), "x", "-idq", "-o+", str(archive), str(output) + "\\"]
    else:
        raise ValueError(method)

    compress_seconds = timed(compress)
    archive_bytes = archive.stat().st_size
    extract_seconds = timed(extract)
    extracted = output / source.name
    verified = extracted.exists() and sha256(extracted) == HASHES[source.name]
    if not verified:
        raise RuntimeError(f"SHA-256 mismatch: {source.name}, {method}")
    result = {
        "source": source.name,
        "round": round_number,
        "method": method,
        "source_bytes": source.stat().st_size,
        "archive_bytes": archive_bytes,
        "compress_seconds": round(compress_seconds, 3),
        "extract_seconds": round(extract_seconds, 3),
        "sha256_match": verified,
    }
    print(result, flush=True)
    verified_remove(output)
    verified_remove(archive)
    return result


if __name__ == "__main__":
    sources = [ROOT / name for name in (sys.argv[1:] or ["mixed.bin", "real2g.bin", "zeros256.bin"])]
    generated = []
    for source in sources:
        if source.name == "zeros256.bin" and not source.exists():
            with source.open("wb") as stream:
                for _ in range(32):
                    stream.write(bytes(8 * 1024 * 1024))
            generated.append(source)
    WORK.mkdir(exist_ok=True)
    HASHES = {source.name: sha256(source) for source in sources}
    fieldnames = ["source", "round", "method", "source_bytes", "archive_bytes", "compress_seconds", "extract_seconds", "sha256_match"]
    with RESULTS.open("w", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=fieldnames)
        writer.writeheader()
        for source in sources:
            rounds = 2 if source.name == "mixed.bin" else 1
            for round_number in range(1, rounds + 1):
                methods = ["hacc_gpu", "hacc_cpu", "zip", "rar"]
                if round_number % 2 == 0:
                    methods.reverse()
                for method in methods:
                    writer.writerow(benchmark(source, method, round_number))
                    stream.flush()
    for source in generated:
        source.unlink()
