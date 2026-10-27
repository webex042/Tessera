"""Generate the synthetic redaction corpora (plan §6.3, §18.1; Stage 1 exit criterion).

    python tools/gen_redaction_corpus.py

Writes tests/fixtures/redaction/secrets.jsonl (every listed secret must be redacted) and
clean.txt (nothing should be redacted). Everything is random and fake: card numbers are
Luhn-valid but unissued-style random digits, keys are random characters in the providers'
formats, and no value belongs to anyone. Luhn and Verhoeff are implemented here independently of
the Rust code, so a checksum bug on one side cannot hide itself. Fixed seed: reproducible.
"""
from __future__ import annotations

import base64
import json
import random
import string
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "tests" / "fixtures" / "redaction"
rng = random.Random(20261026)

D = string.digits
AN = string.ascii_letters + string.digits
UP = string.ascii_uppercase + string.digits


def rnd(chars: str, n: int) -> str:
    return "".join(rng.choice(chars) for _ in range(n))


def luhn_check_digit(body: str) -> str:
    total = 0
    for i, ch in enumerate(reversed(body)):
        d = int(ch)
        if i % 2 == 0:
            d *= 2
            if d > 9:
                d -= 9
        total += d
    return str((10 - total % 10) % 10)


VD = [[0,1,2,3,4,5,6,7,8,9],[1,2,3,4,0,6,7,8,9,5],[2,3,4,0,1,7,8,9,5,6],[3,4,0,1,2,8,9,5,6,7],
      [4,0,1,2,3,9,5,6,7,8],[5,9,8,7,6,0,4,3,2,1],[6,5,9,8,7,1,0,4,3,2],[7,6,5,9,8,2,1,0,4,3],
      [8,7,6,5,9,3,2,1,0,4],[9,8,7,6,5,4,3,2,1,0]]
VP = [[0,1,2,3,4,5,6,7,8,9],[1,5,7,6,2,8,3,0,9,4],[5,8,0,3,7,9,6,1,4,2],[8,9,1,6,0,4,3,5,2,7],
      [9,4,5,3,1,2,6,8,7,0],[4,2,8,6,5,7,3,9,0,1],[2,7,9,3,8,0,6,4,1,5],[7,0,4,6,9,1,3,2,5,8]]
VINV = [0,4,3,2,1,5,6,7,8,9]


def verhoeff_check_digit(body: str) -> str:
    c = 0
    for i, ch in enumerate(reversed(body)):
        c = VD[c][VP[(i + 1) % 8][int(ch)]]
    return str(VINV[c])


def verhoeff_ok(num: str) -> bool:
    c = 0
    for i, ch in enumerate(reversed(num)):
        c = VD[c][VP[i % 8][int(ch)]]
    return c == 0


def card(prefix: str, length: int) -> str:
    body = prefix + rnd(D, length - len(prefix) - 1)
    return body + luhn_check_digit(body)


def group(num: str, sizes: list[int], sep: str) -> str:
    out, i = [], 0
    for s in sizes:
        out.append(num[i:i + s])
        i += s
    return sep.join(out)


def aadhaar() -> str:
    body = rng.choice("23456789") + rnd(D, 10)
    n = body + verhoeff_check_digit(body)
    assert verhoeff_ok(n)
    return n


def b64url(b: bytes) -> str:
    return base64.urlsafe_b64encode(b).rstrip(b"=").decode()


positives: list[dict] = []


def add(kind: str, text: str, *secrets: str) -> None:
    for s in secrets:
        assert s in text, (kind, s, text)
    positives.append({"id": f"{kind}-{sum(1 for p in positives if p['kind'] == kind) + 1:03d}", "kind": kind,
                      "text": text, "secrets": list(secrets)})


# --- cards (40) ---------------------------------------------------------------------------------
networks = [("4", 16, [4, 4, 4, 4]), ("51", 16, [4, 4, 4, 4]), ("55", 16, [4, 4, 4, 4]), ("2221", 16, [4, 4, 4, 4]),
            ("2720", 16, [4, 4, 4, 4]), ("34", 15, [4, 6, 5]), ("37", 15, [4, 6, 5]), ("6011", 16, [4, 4, 4, 4]),
            ("60", 16, [4, 4, 4, 4]), ("65", 16, [4, 4, 4, 4]), ("81", 16, [4, 4, 4, 4]), ("82", 16, [4, 4, 4, 4]),
            ("3530", 16, [4, 4, 4, 4]), ("62", 16, [4, 4, 4, 4])]
card_tpl = ["Card number: {v}", "pay with {v} exp 08/29 cvv in DM", "my visa {v}", "Debit card {v} expires 11/28",
            "use this card {v} for the booking", "{v}", "CC: {v} (company card)", "Kal is card se pay kar dena: {v}",
            "Saved card ending ...  full no {v}", "billing → {v}"]
for i in range(40):
    pre, ln, sizes = networks[i % len(networks)]
    n = card(pre, ln)
    v = [n, group(n, sizes, " "), group(n, sizes, "-")][i % 3]
    add("card", card_tpl[i % len(card_tpl)].format(v=v), v)

# --- aadhaar (30): grouped always; bare only next to a keyword ----------------------------------
aad_tpl_grouped = ["Aadhaar: {v}", "UID {v}", "आधार संख्या {v}", "my aadhar no is {v} pls verify", "{v}",
                   "e-KYC details: name Asha, {v}"]
aad_tpl_bare = ["Aadhaar number {v}", "aadhar {v}", "UIDAI ref: {v}", "आधार {v}"]
for i in range(30):
    n = aadhaar()
    if i % 3 == 2:
        add("aadhaar", aad_tpl_bare[i % len(aad_tpl_bare)].format(v=n), n)
    else:
        v = group(n, [4, 4, 4], " " if i % 2 == 0 else "-")
        add("aadhaar", aad_tpl_grouped[i % len(aad_tpl_grouped)].format(v=v), v)

# --- PAN (20) ------------------------------------------------------------------------------------
for i in range(20):
    v = rnd(string.ascii_uppercase, 3) + rng.choice("PCHFATBLJG") + rnd(string.ascii_uppercase, 1) + rnd(D, 4) + rnd(string.ascii_uppercase, 1)
    add("pan", rng.choice(["PAN: {v}", "my PAN card no {v}", "PAN {v} for TDS", "Form 16 — PAN {v}", "{v} (PAN)"]).format(v=v), v)

# --- bank account + IFSC (20) -------------------------------------------------------------------
for i in range(20):
    acc = rng.choice("123456789") + rnd(D, rng.randint(8, 16))
    ifsc = rng.choice(["HDFC", "ICIC", "SBIN", "UTIB", "KKBK", "PUNB", "BARB", "YESB"]) + "0" + rnd(UP, 6)
    t = rng.choice(["A/c no: {a}, IFSC {i}", "Account Number - {a} / IFSC: {i}", "acct #{a} ({i})",
                    "Transfer to account {a}, IFSC code {i}", "khata sankhya {a} IFSC {i}"])
    add("account", t.format(a=acc, i=ifsc), acc, ifsc)

# --- UPI (20) -----------------------------------------------------------------------------------
handles = ["okaxis", "okhdfcbank", "okicici", "oksbi", "ybl", "ibl", "axl", "paytm", "upi", "apl", "axisbank", "icici"]
for i in range(20):
    v = rng.choice(["asha.verma", "ravi.iyer92", "meera_n", "9876543210", "karan.k", "shop.ganesh"]) + str(rng.randint(1, 99)) + "@" + handles[i % len(handles)]
    add("upi", rng.choice(["pay {v}", "UPI: {v}", "send 1500 to {v} by Friday", "GPay kar do {v} pe", "VPA {v}"]).format(v=v), v)

# --- provider API keys (54) ----------------------------------------------------------------------
keys = [
    lambda: rng.choice(["AKIA", "ASIA"]) + rnd(UP, 16),
    lambda: "ghp_" + rnd(AN, 36),
    lambda: "gho_" + rnd(AN, 36),
    lambda: "github_pat_" + rnd(AN + "_", 59),
    lambda: "xoxb-" + rnd(D, 12) + "-" + rnd(D, 13) + "-" + rnd(AN, 24),
    lambda: "sk-proj-" + rnd(AN + "_-", 48),
    lambda: "sk-ant-api03-" + rnd(AN + "_-", 60),
    lambda: "AIza" + rnd(AN + "_-", 35),
    lambda: "sk_live_" + rnd(AN, 24),
    lambda: "rk_test_" + rnd(AN, 24),
    lambda: "glpat-" + rnd(AN + "_-", 20),
    lambda: "npm_" + rnd(AN, 36),
]
key_tpl = ["export API_KEY={v}", "here's the key {v} don't share", "{v}", "config: {v}", "token for staging: {v}",
           "OPENAI_API_KEY=\"{v}\"", "aws_access_key_id = {v}", "pasted by mistake -> {v}"]
for i in range(54):
    v = keys[i % len(keys)]()
    add("api_key", key_tpl[i % len(key_tpl)].format(v=v), v)

# --- JWT (10) -----------------------------------------------------------------------------------
for i in range(10):
    head = b64url(json.dumps({"alg": "HS256", "typ": "JWT"}).encode())
    body = b64url(json.dumps({"sub": rnd(D, 8), "exp": 1790000000 + i}).encode())
    v = f"{head}.{body}.{b64url(rng.randbytes(32))}"
    add("jwt", rng.choice(["Authorization: Bearer {v}", "cookie session={v}", "jwt {v}"]).format(v=v), v)

# --- PEM private keys (6) ----------------------------------------------------------------------
for i, kind in enumerate(["RSA PRIVATE KEY", "PRIVATE KEY", "EC PRIVATE KEY", "OPENSSH PRIVATE KEY", "RSA PRIVATE KEY", "PRIVATE KEY"]):
    lines = [base64.b64encode(rng.randbytes(48)).decode() for _ in range(4)]
    body = "\n".join(lines)
    v = f"-----BEGIN {kind}-----\n{body}\n-----END {kind}-----"
    add("private_key", f"deploy key below\n{v}\nthanks", *lines)

# --- generic secrets next to keywords (24) -----------------------------------------------------
for i in range(24):
    v = rng.choice([rnd("0123456789abcdef", 32), rnd("0123456789abcdef", 40), rnd(AN + "+/", 40), rnd(AN, 28) + "=="])
    t = rng.choice(["api_key = {v}", "SECRET_KEY: {v}", "client_secret={v}", "Authorization: Bearer {v}",
                    "the token is {v}", "access_key: '{v}'", "private_key = \"{v}\"", "secret = {v}"])
    add("secret", t.format(v=v), v)

# --- OTPs (40) ----------------------------------------------------------------------------------
otp_tpl = ["Your OTP is {v}. Do not share it with anyone.", "{v} is your verification code for Northwind",
           "G-{v} is your Google verification code", "Your Northwind sign-in code is {v}",
           "OTP {v} hai, kisi ko mat batana", "aapka OTP {v} hai", "Enter {v} as your one-time password",
           "आपका ओटीपी {v} है", "Use verification code: {v} to continue", "Login code: {v}",
           "{v} hai aapka OTP for HDFC netbanking", "security code {v} (valid 10 min)"]
for i in range(40):
    v = rnd(D, rng.choice([4, 6, 6, 6, 8]))
    if v[0] == "0":
        v = "7" + v[1:]
    add("otp", otp_tpl[i % len(otp_tpl)].format(v=v), v)

# --- passwords (26) -----------------------------------------------------------------------------
for i in range(26):
    v = rnd(string.ascii_letters, 5) + rnd(D, 2) + rng.choice("!@#$%&*") + rnd(AN, 3)
    t = rng.choice(["password: {v}", "Password = {v}", "pwd: {v}", "wifi password is {v}", "password hai {v}",
                    "पासवर्ड: {v}", "new passcode: {v}", "the admin password is \"{v}\""])
    add("password", t.format(v=v), v)

# --- clean corpus -------------------------------------------------------------------------------
# --- Stage 7 additions: more Indian identifiers (own RNG, so earlier cases are unchanged) --------
rng7 = random.Random(20261027)


def vid() -> str:
    body = str(rng7.randint(2, 9)) + "".join(str(rng7.randint(0, 9)) for _ in range(14))
    return body + verhoeff_check_digit(body)


for _ in range(20):
    v = vid()
    shown = group(v, [4, 4, 4, 4], rng7.choice([" ", "-", ""]))
    t = rng7.choice(["Aadhaar VID: {v}", "virtual ID {v} for e-KYC", "UIDAI VID {v}", "Your VID is {v}. Do not share it."])
    add("aadhaar", t.format(v=shown), shown)

for _ in range(15):
    acc = "".join(str(rng7.randint(0, 9)) for _ in range(rng7.randint(11, 16)))
    bank = rng7.choice(["SBIN", "HDFC", "ICIC", "UTIB", "KKBK", "PUNB"])
    ifsc = bank + "0" + "".join(rng7.choice("0123456789") for _ in range(6))
    t = rng7.choice([
        "Beneficiary: Meera Shah, A/c {a}, IFSC {i}",
        "acc no. {a} ({i})",
        "NEFT details - Account Number: {a} | IFSC: {i}",
        "khata number {a}, IFSC {i} pe bhej dena",
    ])
    add("account", t.format(a=acc, i=ifsc), acc, ifsc)

for _ in range(15):
    pp = rng7.choice("ABCDEFGHJKLMNPRSTUVWZ") + str(rng7.randint(1, 9)) + "".join(str(rng7.randint(0, 9)) for _ in range(6))
    t = rng7.choice(["Passport No: {v}", "passport number {v}, expires 2031", "Passport #{v} (scan attached)", "pp no. {v}"])
    add("passport", t.format(v=pp), pp)

for _ in range(15):
    epic = "".join(rng7.choice("ABCDEFGHJKLMNPRSTUVWXYZ") for _ in range(3)) + "".join(str(rng7.randint(0, 9)) for _ in range(7))
    t = rng7.choice(["Voter ID: {v}", "EPIC no. {v}", "voter id card number {v}", "Election card {v} for address proof"])
    add("voter_id", t.format(v=epic), epic)


clean: list[str] = []
months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
for i in range(40):
    clean.append(f"Order #{rng.choice('123456789')}{rnd(D, rng.choice([9, 11, 15]))} shipped on {rng.randint(1, 28)} {rng.choice(months)}")
for i in range(15):
    clean.append(f"Tracking id {rnd(D, 12)} - out for delivery")  # bare 12 digits, no Aadhaar keyword
for i in range(15):
    clean.append(f"Invoice INV-{rnd(D, 5)} for INR {rng.randint(1, 9)},{rnd(D, 2)},{rnd(D, 3)} is due on {rng.randint(1, 28):02d}/{rng.randint(1, 12):02d}/2026")
for i in range(15):
    clean.append(f"commit {rnd('0123456789abcdef', 40)} merged into main")
for i in range(10):
    clean.append(f"request id {rnd('0123456789abcdef', 8)}-{rnd('0123456789abcdef', 4)}-{rnd('0123456789abcdef', 4)}-{rnd('0123456789abcdef', 4)}-{rnd('0123456789abcdef', 12)}")
for i in range(10):
    clean.append(f"Build #{rng.randint(10000, 99999)} passed in {rng.randint(1, 59)}m {rng.randint(1, 59)}s on runner {rnd(string.ascii_lowercase, 6)}")
for i in range(10):
    clean.append(f"Meeting at {rng.randint(1, 12)}:{rng.choice(['00', '15', '30', '45'])} pm, room {rng.randint(100, 999)}, call +91 {rng.choice('6789')}{rnd(D, 4)} {rnd(D, 5)}")
for i in range(10):
    clean.append(f"v{rng.randint(1, 9)}.{rng.randint(0, 30)}.{rng.randint(0, 99)} released; {rng.randint(10, 500)} tests, p95 {rng.randint(10, 900)} ms")
prose = [
    "the token bucket algorithm limits requests per second",
    "Password policy: at least 12 characters, rotated every 90 days",
    "OTP will be sent to your registered mobile shortly",
    "Reset your password from the account settings page",
    "We need a secret santa list for the offsite",
    "Key results for Q4: activation 38% -> 50%",
    "Please share your PAN card copy with HR (do not paste the number here)",
    "Your Aadhaar must be linked to your PAN before 31 March",
    "API keys are stored in the vault, never in Slack",
    "The authorization flow uses PKCE; tokens expire after 3600 seconds",
    "card sorting workshop on Thursday at 4 pm",
    "account manager: Arjun; account health score 82",
    "Sprint 45 ends 30 Oct; 11 tickets done, 4 in review",
    "Total travel 22,50,000 vs 23,55,000 actual (+4.7%)",
    "Diwali party venue: 4th floor, 6:30 pm onwards",
    "kal subah 10 baje standup ke baad call karna",
    "रविवार को मामा जी के घर लंच है",
    "ISBN 978-0-13-468599-1 is on the reading list",
    "Coordinates 12.9716, 77.5946 (Bengaluru)",
    "Server 10.0.12.45 port 8443 is healthy",
    "Room booking ref BK-2026-10-0042",
    "Use the code review checklist before merging",
    "verification code will arrive by SMS",
    "OTP verification failed; please try again",
    "secret: see vault entry northwind/prod/db",
    "token: <redacted by user>",
    "password: ********",
    "PIN code 560038 (Indiranagar)",
    "Pay 1500 to Karan by Friday for the reunion",
    "Product SKU 4006381333931 restocked",
    "Flight AI2207 departs 06:40 from gate B12",
    "Invoice INV2026001234 is due on 30 Oct",
    "The passport office opens at 9:30; bring the old passport and two photos",
    "Voter list revision camp on Sunday at the school",
    "Order ID 4021-7788-1201-5530 shipped",
    "Ticket ABC1234 assigned to the infra team",
    "Model K1234 router firmware 2.3.7",
]
clean.extend(prose)

OUT.mkdir(parents=True, exist_ok=True)
with open(OUT / "secrets.jsonl", "w", encoding="utf-8", newline="\n") as f:
    for p in positives:
        f.write(json.dumps(p, ensure_ascii=False) + "\n")
(OUT / "clean.txt").write_text("\n".join(clean) + "\n", encoding="utf-8", newline="\n")
counts: dict[str, int] = {}
for p in positives:
    counts[p["kind"]] = counts.get(p["kind"], 0) + 1
print(f"positives: {len(positives)} {counts}")
print(f"clean lines: {len(clean)}")
