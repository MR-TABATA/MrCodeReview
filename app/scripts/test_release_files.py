"""release_files.py の試験（ネットワークも、署名の鍵も使わない。一時フォルダだけ）。"""
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import release_files as rf  # noqa: E402

NAME = "MrCodeReview"


def read_json(path):
    with open(path) as fh:
        return json.load(fh)


def read_bytes(path):
    with open(path, "rb") as fh:
        return fh.read()


def put(site, version, kinds=("dmg", "tgz")):
    made = []
    for k in kinds:
        f = f"{NAME}_{version}_aarch64.dmg" if k == "dmg" else f"{NAME}_{version}.app.tar.gz"
        with open(os.path.join(site, f), "wb") as fh:
            fh.write(f"{f}".encode())
        made.append(f)
    return made


class Site(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.site = self._tmp.name
        self.addCleanup(self._tmp.cleanup)

    def names(self):
        return sorted(os.listdir(self.site))


class PruneTests(Site):
    def test_versions_are_compared_as_numbers_so_only_0_9_0_goes_with_keep_3(self):
        for v in ("0.9.0", "0.10.0", "1.0.0", "1.0.1"):
            put(self.site, v)
        removed = rf.prune_old_versions(self.site, NAME, 3)
        self.assertEqual(sorted(removed), [f"{NAME}_0.9.0.app.tar.gz", f"{NAME}_0.9.0_aarch64.dmg"])
        self.assertEqual(len(self.names()), 6)
        for v in ("0.10.0", "1.0.0", "1.0.1"):
            self.assertIn(f"{NAME}_{v}_aarch64.dmg", self.names())
            self.assertIn(f"{NAME}_{v}.app.tar.gz", self.names())

    def test_the_newest_is_never_removed_even_when_a_string_sort_would_put_it_last(self):
        for v in ("0.2.0", "0.10.0", "0.9.0"):
            put(self.site, v)
        rf.prune_old_versions(self.site, NAME, 1)
        self.assertEqual(self.names(), [f"{NAME}_0.10.0.app.tar.gz", f"{NAME}_0.10.0_aarch64.dmg"])

    def test_nothing_is_removed_when_there_are_not_more_versions_than_keep(self):
        for v in ("1.0.0", "1.0.1"):
            put(self.site, v)
        self.assertEqual(rf.prune_old_versions(self.site, NAME, 3), [])
        self.assertEqual(len(self.names()), 4)

    def test_unrelated_files_are_left_alone(self):
        for v in ("1.0.0", "1.0.1", "1.0.2"):
            put(self.site, v)
        others = ["update.json", "latest.json", "Other_0.1.0_aarch64.dmg", f"{NAME}_0.0.1_universal.dmg", f"{NAME}_0.0.1.zip", "README.md"]
        for f in others:
            open(os.path.join(self.site, f), "w").close()
        rf.prune_old_versions(self.site, NAME, 1)
        for f in others:
            self.assertIn(f, self.names())

    def test_a_keep_below_one_is_refused_and_removes_nothing(self):
        put(self.site, "1.0.0")
        for bad in (0, -1):
            with self.assertRaises(ValueError):
                rf.prune_old_versions(self.site, NAME, bad)
        self.assertEqual(len(self.names()), 2)

    def test_the_version_just_released_is_protected_even_if_it_is_older_than_the_others(self):
        for v in ("1.0.0", "1.0.1", "1.0.2"):
            put(self.site, v)
        rf.prune_old_versions(self.site, NAME, 1, protect="1.0.0")
        self.assertIn(f"{NAME}_1.0.0_aarch64.dmg", self.names(), "update.json が指す版は、消さない")
        self.assertIn(f"{NAME}_1.0.2_aarch64.dmg", self.names())
        self.assertNotIn(f"{NAME}_1.0.1_aarch64.dmg", self.names())

    def test_the_command_line_prunes_and_refuses_keep_0(self):
        for v in ("1.0.0", "1.0.1"):
            put(self.site, v)
        script = os.path.join(HERE, "release_files.py")
        bad = subprocess.run([sys.executable, script, "prune", self.site, NAME, "0"], capture_output=True, text=True)
        self.assertEqual(bad.returncode, 1)
        self.assertEqual(len(self.names()), 4)
        ok = subprocess.run([sys.executable, script, "prune", self.site, NAME, "1", "--protect", "1.0.1"], capture_output=True, text=True)
        self.assertEqual(ok.returncode, 0, ok.stderr)
        self.assertIn("消した:", ok.stdout)
        self.assertEqual(len(self.names()), 2)


class PruneBadKeepTests(Site):
    def test_a_non_numeric_keep_stops_before_touching_anything(self):
        for v in ("1.0.0", "1.0.1"):
            put(self.site, v)
        script = os.path.join(HERE, "release_files.py")
        for bad in ("abc", "", "1.5", "-1", "0"):
            r = subprocess.run([sys.executable, script, "prune", self.site, NAME, bad], capture_output=True, text=True)
            self.assertNotEqual(r.returncode, 0, f"keep={bad!r}")
            self.assertEqual(len(self.names()), 4, f"keep={bad!r}: 何も消さない")


class WriteTests(Site):
    def make(self, version="1.0.1", sig="  SIGNATURE-BASE64==\n"):
        dmg = f"{NAME}_{version}_aarch64.dmg"
        tgz = f"{NAME}_{version}.app.tar.gz"
        with open(os.path.join(self.site, dmg), "wb") as fh:
            fh.write(b"dmg-bytes-" * 100)
        sig_file = os.path.join(self.site, f"{tgz}.sig")
        with open(sig_file, "w") as fh:
            fh.write(sig)
        return dmg, tgz, sig_file

    def test_update_json_has_the_version_url_and_trimmed_signature_for_apple_silicon_only(self):
        dmg, tgz, sig = self.make()
        update, _ = rf.write_update_files(self.site, "1.0.1", dmg, tgz, "https://mrktools.app/downloads/mrcodereview", sig, NAME, now="2026-10-30T00:00:00Z")
        on_disk = read_json(os.path.join(self.site, "update.json"))
        self.assertEqual(on_disk, update)
        self.assertEqual(on_disk["version"], "1.0.1")
        self.assertEqual(on_disk["pub_date"], "2026-10-30T00:00:00Z")
        self.assertEqual(list(on_disk["platforms"]), ["darwin-aarch64"])
        p = on_disk["platforms"]["darwin-aarch64"]
        self.assertEqual(p["signature"], "SIGNATURE-BASE64==", "前後の空白・改行は除く")
        self.assertEqual(p["url"], f"https://mrktools.app/downloads/mrcodereview/{tgz}")

    def test_latest_json_has_the_real_size_and_sha256_of_the_dmg(self):
        dmg, tgz, sig = self.make()
        _, latest = rf.write_update_files(self.site, "1.0.1", dmg, tgz, "https://example.test/d", sig, NAME, now="2026-10-30T00:00:00Z")
        data = read_bytes(os.path.join(self.site, dmg))
        f = read_json(os.path.join(self.site, "latest.json"))["files"][0]
        self.assertEqual((f["name"], f["size"], f["sha256"]), (dmg, len(data), hashlib.sha256(data).hexdigest()))
        self.assertEqual((latest["version"], latest["tag"]), ("1.0.1", "v1.0.1"))

    def test_a_missing_dmg_or_signature_fails_instead_of_writing_a_broken_update(self):
        dmg, tgz, sig = self.make()
        os.remove(os.path.join(self.site, dmg))
        with self.assertRaises(FileNotFoundError):
            rf.write_update_files(self.site, "1.0.1", dmg, tgz, "https://example.test/d", sig, NAME)
        self.assertNotIn("update.json", self.names())

    def test_a_missing_signature_file_writes_nothing(self):
        dmg, tgz, sig = self.make()
        os.remove(sig)
        with self.assertRaises(FileNotFoundError):
            rf.write_update_files(self.site, "1.0.1", dmg, tgz, "https://example.test/d", sig, NAME)
        self.assertNotIn("update.json", self.names())
        self.assertNotIn("latest.json", self.names())




ROOT = os.path.dirname(HERE)
CONF = os.path.join(ROOT, "src-tauri", "tauri.conf.json")


class ConfigTests(unittest.TestCase):
    def test_the_real_config_passes(self):
        self.assertEqual(rf.check_config(CONF, "MrCodeReview", "com.hitoshi.mrcodereview", "mrcodereview"), [])

    def test_a_wrong_name_identifier_or_endpoint_is_reported(self):
        problems = rf.check_config(CONF, "Other", "com.example.other", "other")
        self.assertEqual(len(problems), 3, problems)
        self.assertTrue(any("productName" in p for p in problems) and any("identifier" in p for p in problems) and any("更新の宛先" in p for p in problems))

    def test_a_missing_icon_and_a_missing_endpoint_are_reported(self):
        with tempfile.TemporaryDirectory() as d:
            conf = os.path.join(d, "tauri.conf.json")
            with open(conf, "w") as fh:
                json.dump({"productName": "A", "identifier": "x.a", "bundle": {"icon": ["icons/none.icns"]}}, fh)
            problems = rf.check_config(conf, "A", "x.a", "a")
            self.assertTrue(any("アイコンのファイルがありません" in p for p in problems), problems)
            self.assertTrue(any("endpoints がありません" in p for p in problems), problems)

    def test_the_command_line_exits_1_on_a_problem_and_0_otherwise(self):
        script = os.path.join(HERE, "release_files.py")
        good = subprocess.run([sys.executable, "-I", script, "check-config", CONF, "MrCodeReview", "com.hitoshi.mrcodereview", "mrcodereview"], capture_output=True)
        bad = subprocess.run([sys.executable, "-I", script, "check-config", CONF, "Other", "com.hitoshi.mrcodereview", "mrcodereview"], capture_output=True)
        self.assertEqual((good.returncode, bad.returncode), (0, 1))


if __name__ == "__main__":
    unittest.main()
