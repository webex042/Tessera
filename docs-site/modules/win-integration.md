# OS Integration (`tsr-win`)

The `tsr-win` crate provides safe Rust wrappers around Windows Win32 and WinRT system APIs. It manages cryptographic key protection, job containment, event hooks, and TCP query interfaces.

---

## Key Modules

### 1. DPAPI Key Derivation (`tsr_win::dpapi`)
Protects the master 256-bit SQLCipher encryption key using the Windows Data Protection API:
- `CryptProtectData`: Encrypts the raw key with the user's Windows login credentials before saving to `%LOCALAPPDATA%\Tessara\key.bin`.
- `CryptUnprotectData`: Unwraps the key into a `Zeroizing<[u8; 32]>` buffer on application launch.
- If the database file exists but the key file is missing, Tessera halts immediately to prevent data corruption.

### 2. Job Object Containment (`tsr_win::job`)
Encapsulates external sidecar processes:
- Creates a kernel Job Object via `CreateJobObjectW`.
- Configures `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
- Guarantees child termination upon host crash or process termination.

### 3. Foreground & System Event Hooks (`tsr_win::events`)
Operates the event-driven capture loop on a dedicated thread (T1):
- `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, ...)`: Fires whenever the active desktop focus shifts.
- `EVENT_OBJECT_NAMECHANGE`: Registered selectively for the active foreground process to capture tab switches without system-wide hook overhead.
- A hidden message-only window listens for:
  - `WM_WTSSESSION_CHANGE`: Detects session lock (`WTS_SESSION_LOCK`) and unlock.
  - `WM_POWERBROADCAST`: Detects AC/battery transitions (`GUID_ACDC_POWER_SOURCE`).

### 4. TCP Port Owner Verification (`tsr_win::tcp`)
Inspects active loopback sockets using Win32 `GetExtendedTcpTable`:
- Queries the PID of the process listening on `127.0.0.1:<port>`.
- Confirms that the listening socket belongs to the authorized sidecar child process before any HTTP connection is attempted.
