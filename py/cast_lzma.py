import lzma
import os
import shutil
import subprocess
import random
import sys
from typing import Optional


# ============================================================================
#  HELPER: 7-Zip Detection
# ============================================================================

def get_7z_cmd() -> str:
    env_path = os.environ.get("SEVEN_ZIP_PATH")
    if env_path:
        return env_path.strip('"')

    if sys.platform == "win32":
        standard = r"C:\Program Files\7-Zip\7z.exe"
        if os.path.exists(standard):
            return standard
        return "7z.exe"

    elif sys.platform == "darwin":
        common_paths = [
            "/opt/homebrew/bin/7zz",
            "/usr/local/bin/7zz",
            "/usr/local/bin/7z"
        ]
        for p in common_paths:
            if os.path.exists(p):
                return p
        return "7zz"

    return "7z"


def try_find_7zip_path() -> Optional[str]:
    cmd = get_7z_cmd()

    # Check if absolute path exists
    if os.path.isabs(cmd):
        if os.path.exists(cmd):
            return cmd
        return None

    # Check if command is in PATH
    resolved = shutil.which(cmd)
    if resolved:
        return resolved

    return None


# ============================================================================
#  BACKEND 1: NATIVE (LZMA Lib)
# ============================================================================

class LzmaBackend:
    def __init__(self, dict_size: Optional[int] = None):
        self.dict_size = dict_size

    def compress(self, data: bytes) -> bytes:
        if not data:
            return b""

        if self.dict_size is not None:
            custom_filters = [{
                "id": lzma.FILTER_LZMA2,
                "preset": 9 | lzma.PRESET_EXTREME,
                "dict_size": self.dict_size
            }]
            return lzma.compress(data, check=lzma.CHECK_CRC32, filters=custom_filters)
        else:
            return lzma.compress(data, preset=9 | lzma.PRESET_EXTREME)


class LzmaDecompressorBackend:
    def decompress(self, data: bytes) -> bytes:
        if not data:
            return b""
        return lzma.decompress(data)


# ============================================================================
#  BACKEND 2: 7-ZIP (External Executable)
# ============================================================================

class SevenZipBackend:
    def __init__(self, dict_size: Optional[int] = None):
        # Default dict size handling inside 7zip logic if None passed
        self.dict_size = dict_size if dict_size is not None else 128 * 1024 * 1024

    def compress(self, data: bytes) -> bytes:
        if not data:
            return b""

        # Create temporary files
        pid = os.getpid()
        rnd = random.randint(0, 1000000)
        tmp_in = f"temp_in_{pid}_{rnd}.bin"
        tmp_out = f"temp_out_{pid}_{rnd}.xz"

        try:
            with open(tmp_in, "wb") as f:
                f.write(data)
                f.flush()
                os.fsync(f.fileno())

            # Construct 7z command
            # -m0=lzma2:d{size}b
            dict_arg = f"-m0=lzma2:d{self.dict_size}b"
            cmd_path = get_7z_cmd()

            args = [
                cmd_path,
                "a",
                "-txz",
                "-mx=9",
                "-mmt=on",
                dict_arg,
                "-y",
                "-bb0",
                tmp_out,
                tmp_in
            ]

            # Execute
            # Using subprocess.run to be safe and clean
            subprocess.run(args, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)

            with open(tmp_out, "rb") as f:
                result = f.read()

            return result

        except subprocess.CalledProcessError as e:
            # Fallback logic was inside the old class, but here we raise
            # so the caller knows 7zip failed. Or we could print error.
            # Following the old logic: "7z Backend failed... Falling back"
            # But since this is a specific backend class, it should probably fail
            # and let the runtime wrapper handle fallback or just fail.
            # For strict adherence to "don't change logic", the old code caught exception
            # and fell back to lzma.compress.
            # However, in this architecture, SevenZipBackend is explicit.
            # I will raise RuntimeError to signal failure.
            error_msg = e.stderr.decode('utf-8', errors='ignore') if e.stderr else str(e)
            raise RuntimeError(f"7-Zip Error: {error_msg}")
        finally:
            # Cleanup
            if os.path.exists(tmp_in): os.remove(tmp_in)
            if os.path.exists(tmp_out): os.remove(tmp_out)


class SevenZipDecompressorBackend:
    def decompress(self, data: bytes) -> bytes:
        if not data:
            return b""

        pid = os.getpid()
        rnd = random.randint(0, 1000000)
        tmp_in = f"temp_dec_in_{pid}_{rnd}.xz"

        try:
            with open(tmp_in, "wb") as f:
                f.write(data)
                f.flush()
                os.fsync(f.fileno())

            cmd_path = get_7z_cmd()
            args = [cmd_path, "e", tmp_in, "-so", "-y"]

            # Execute and capture stdout
            proc = subprocess.run(args, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            return proc.stdout

        except subprocess.CalledProcessError as e:
            error_msg = e.stderr.decode('utf-8', errors='ignore') if e.stderr else str(e)
            print(f"Decompression Error: {error_msg}")
            return b""
        finally:
            if os.path.exists(tmp_in): os.remove(tmp_in)


# ============================================================================
#  RUNTIME WRAPPERS (Dynamic Switching)
# ============================================================================

class RuntimeLzmaCompressor:
    def __init__(self, backend_type: str, dict_size: Optional[int] = None):
        if backend_type == "7zip":
            self.backend = SevenZipBackend(dict_size)
        else:
            self.backend = LzmaBackend(dict_size)

    def compress(self, data: bytes) -> bytes:
        # Note: The old code had a fallback inside 7z_compress.
        # If we want strictly identical behavior, we should wrap try-except here.
        if isinstance(self.backend, SevenZipBackend):
            try:
                return self.backend.compress(data)
            except Exception as e:
                print(f"[!] 7z Backend failed ({e}). Falling back to native LZMA.")
                # Fallback to native
                return LzmaBackend(self.backend.dict_size).compress(data)
        else:
            return self.backend.compress(data)


class RuntimeLzmaDecompressor:
    def __init__(self, backend_type: str):
        if backend_type == "7zip":
            self.backend = SevenZipDecompressorBackend()
        else:
            self.backend = LzmaDecompressorBackend()

    def decompress(self, data: bytes) -> bytes:
        # Similar fallback logic for decompression could be added if desired,
        # but the old code simply returned b"" on decompression error or
        # had a fallback in _decompress_payload only if 7z path was found.
        # Here we stick to the backend's logic.
        if isinstance(self.backend, SevenZipDecompressorBackend):
            try:
                res = self.backend.decompress(data)
                if not res and data:  # If empty result but input wasn't
                    # Fallback check similar to old _decompress_payload
                    return LzmaDecompressorBackend().decompress(data)
                return res
            except:
                return LzmaDecompressorBackend().decompress(data)
        return self.backend.decompress(data)


# ============================================================================
#  TYPE ALIASES
# ============================================================================

CASTLzmaCompressor = RuntimeLzmaCompressor
CASTLzmaDecompressor = RuntimeLzmaDecompressor