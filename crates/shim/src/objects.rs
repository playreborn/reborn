//! The objects the shim hands out: a pointer to a table of functions, as
//! the official client's objects are shaped (`docs/shim/objects.md`, step
//! 3a). Positions not listed here end the process with a log line.

use std::ffi::{c_char, c_void};
use std::sync::{Mutex, OnceLock};

use crate::{log, not_implemented, session, text_arg};

/// Table positions per object; the highest recorded position is 40.
const SLOTS: usize = 48;

/// An object as games see it: its first field points to the table.
#[repr(C)]
struct Object {
    table: *const usize,
}

/// A built object, shareable between threads (it is never written after
/// it is built).
struct Built {
    object: Box<Object>,
    /// The same table, for the tests to read without raw pointers.
    #[cfg_attr(not(test), allow(dead_code))]
    table: &'static [usize; SLOTS],
}
// SAFETY: the object and its table are created once and only read
// afterwards; the pointer targets a leaked, never-freed table.
unsafe impl Send for Built {}
// SAFETY: as above.
unsafe impl Sync for Built {}

impl Built {
    fn new(table: [usize; SLOTS]) -> Self {
        let table: &'static [usize; SLOTS] = Box::leak(Box::new(table));
        Built {
            object: Box::new(Object {
                table: table.as_ptr(),
            }),
            table,
        }
    }

    fn pointer(&self) -> *mut c_void {
        (&*self.object as *const Object).cast_mut().cast()
    }
}

/// One stub per position that logs which position of which object was
/// called and stops.
macro_rules! stubs {
    ($object:literal) => {
        stubs!(@ $object; 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23
               24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47)
    };
    (@ $object:literal; $($n:literal)*) => {
        [$({
            extern "C" fn stub() -> usize {
                not_implemented(concat!($object, " position ", stringify!($n)))
            }
            stub as *const () as usize
        }),*]
    };
}

// ---- SteamClient (meanings.md, experiment 7) ----

/// Handle values Reborn gives out. The official client gave 1 for both in
/// every recorded run; their meaning is only that later calls pass them
/// back (experiment 7).
const PIPE: i32 = 1;
pub(crate) const USER: i32 = 1;

/// `CreateSteamPipe` confirmed in experiment 7.
extern "C" fn create_steam_pipe(_this: *const Object) -> i32 {
    PIPE
}

/// `ConnectToGlobalUser` confirmed in experiment 7.
extern "C" fn connect_to_global_user(_this: *const Object, _pipe: i32) -> i32 {
    USER
}

/// `GetISteamUtils` (pipe, version text), confirmed in experiment 7.
extern "C" fn get_utils_interface(
    _this: *const Object,
    _pipe: i32,
    version: *const c_char,
) -> *mut c_void {
    // SAFETY: games pass a NUL-terminated version text here
    // (`docs/shim/objects.md`).
    interface_or_stop(unsafe { text_arg(version) })
}

/// `GetISteamGenericInterface` and the per-interface `GetISteam…`
/// (user, pipe, version text), confirmed in experiment 7.
extern "C" fn get_user_interface(
    _this: *const Object,
    _user: i32,
    _pipe: i32,
    version: *const c_char,
) -> *mut c_void {
    // SAFETY: as above.
    interface_or_stop(unsafe { text_arg(version) })
}

/// `ReleaseUser`, confirmed (position 4 in 020, 021, 023; experiments 7, 11).
extern "C" fn release_user(_this: *const Object, _pipe: i32, _user: i32) {}

/// `BReleaseSteamPipe`, confirmed (position 1; experiments 7, 11).
extern "C" fn release_steam_pipe(_this: *const Object, _pipe: i32) -> bool {
    true
}

/// `BShutdownIfAllPipesClosed`, confirmed (23 in 020 and 021, 22 in 023;
/// experiments 7, 11).
extern "C" fn shutdown_if_all_pipes_closed(_this: *const Object) -> bool {
    true
}

fn interface_or_stop(version: Option<&str>) -> *mut c_void {
    match version.and_then(interface_for) {
        Some(object) => {
            log(&format!("handed out {}", version.unwrap_or("?")));
            object
        }
        None => not_implemented(&format!("interface {}", version.unwrap_or("?"))),
    }
}

/// Positions that took an interface version text with a user and a pipe
/// handle, per `SteamClient` version (`docs/shim/objects.md`, raw logs of
/// `coverage.md`).
const USER_INTERFACE_POSITIONS_020: &[usize] = &[
    5, 8, 10, 11, 12, 13, 15, 16, 17, 18, 19, 24, 27, 28, 29, 30, 31, 35, 36, 37, 38, 39, 40,
];
const USER_INTERFACE_POSITIONS_021: &[usize] = &[
    5, 8, 10, 11, 12, 13, 15, 16, 17, 18, 19, 24, 26, 27, 28, 29, 33, 34, 35, 36, 37, 38,
];
const USER_INTERFACE_POSITIONS_023: &[usize] = &[
    5, 8, 10, 11, 12, 13, 15, 16, 17, 18, 23, 25, 26, 27, 31, 32, 33, 34, 35, 36,
];

fn client_table(mut table: [usize; SLOTS], user_positions: &[usize]) -> [usize; SLOTS] {
    table[0] = create_steam_pipe as *const () as usize;
    table[2] = connect_to_global_user as *const () as usize;
    table[9] = get_utils_interface as *const () as usize;
    for &position in user_positions {
        table[position] = get_user_interface as *const () as usize;
    }
    table
}

/// The calls at close (experiments 7 and 11): position 4 `ReleaseUser`,
/// 1 `BReleaseSteamPipe`, and `BShutdownIfAllPipesClosed` at `shutdown`
/// (23 in 020 and 021, 22 in 023).
fn add_close_calls(table: &mut [usize; SLOTS], shutdown: usize) {
    table[1] = release_steam_pipe as *const () as usize;
    table[4] = release_user as *const () as usize;
    table[shutdown] = shutdown_if_all_pipes_closed as *const () as usize;
}

fn client_020() -> &'static Built {
    static OBJECT: OnceLock<Built> = OnceLock::new();
    OBJECT.get_or_init(|| {
        let mut table = client_table(stubs!("SteamClient020"), USER_INTERFACE_POSITIONS_020);
        add_close_calls(&mut table, 23);
        Built::new(table)
    })
}

fn client_021() -> &'static Built {
    static OBJECT: OnceLock<Built> = OnceLock::new();
    OBJECT.get_or_init(|| {
        let mut table = client_table(stubs!("SteamClient021"), USER_INTERFACE_POSITIONS_021);
        add_close_calls(&mut table, 23);
        Built::new(table)
    })
}

fn client_023() -> &'static Built {
    static OBJECT: OnceLock<Built> = OnceLock::new();
    OBJECT.get_or_init(|| {
        let mut table = client_table(stubs!("SteamClient023"), USER_INTERFACE_POSITIONS_023);
        add_close_calls(&mut table, 22);
        Built::new(table)
    })
}

// ---- SteamClient018 and SteamClient019 (meanings.md, experiment 24) ----

/// The handle `CreateLocalUser` writes as the second pipe: 2 in every
/// recorded run (experiment 24, C and H).
const GAME_SERVER_PIPE: i32 = 2;

/// The user `CreateLocalUser` returned: 1 in every recorded run
/// (experiment 24, B).
const GAME_SERVER_USER: i32 = 1;

/// `k_EAccountTypeGameServer`, the only account type recorded at
/// position 3 (experiment 24, B; the `EAccountType` table of Valve's
/// `steam_api` page).
const ACCOUNT_TYPE_GAME_SERVER: i32 = 3;

/// The pipes open on the `SteamClient018`/`019` objects, for
/// `BShutdownIfAllPipesClosed` (experiment 24, F: false while the other
/// pipe is still open, true once both are released). A game uses one
/// `SteamClient` version, so one list for both is enough.
static OPEN_PIPES: Mutex<Vec<i32>> = Mutex::new(Vec::new());

fn open_pipes() -> std::sync::MutexGuard<'static, Vec<i32>> {
    OPEN_PIPES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Opens `pipe`; a second open of the same pipe was never recorded.
fn open_pipe(pipe: i32, what: &str) {
    let mut pipes = open_pipes();
    if pipes.contains(&pipe) {
        drop(pipes);
        not_implemented(&format!("{what} while pipe {pipe} is already open"));
    }
    pipes.push(pipe);
}

/// `CreateSteamPipe`, confirmed for 018 and 019 (experiment 24, A):
/// result 1, called once.
extern "C" fn create_steam_pipe_two(_this: *const Object) -> i32 {
    open_pipe(PIPE, "SteamClient018/019 position 0");
    PIPE
}

/// `CreateLocalUser` (pipe address, account type), confirmed for 018 and
/// 019 (experiment 24, B, C, H): writes the new pipe 2 at the address in
/// `rdx`, takes 3 = `k_EAccountTypeGameServer` and returns the user 1.
///
/// The shim has no `SteamGameServer…` object and no objects for the
/// second pipe (the official client handed other objects over it,
/// experiment 24, E), so every getter call on pipe 2 stops
/// (`get_on_first_pipe`). A null address or another account type was
/// never recorded and stops too.
extern "C" fn create_local_user(
    _this: *const Object,
    pipe_out: *mut i32,
    account_type: i32,
) -> i32 {
    if pipe_out.is_null() {
        not_implemented("SteamClient018/019 position 3 with a null pipe address");
    }
    if account_type != ACCOUNT_TYPE_GAME_SERVER {
        not_implemented(&format!(
            "SteamClient018/019 position 3 with account type {account_type}"
        ));
    }
    open_pipe(GAME_SERVER_PIPE, "SteamClient018/019 position 3");
    // SAFETY: `pipe_out` is not null; per Valve's `ISteamClient` page
    // `CreateLocalUser` takes an `HSteamPipe *` there, and the recorded
    // calls passed an address the game later read the pipe from
    // (experiment 24, H). An unaligned write needs no alignment.
    unsafe { pipe_out.write_unaligned(GAME_SERVER_PIPE) };
    GAME_SERVER_USER
}

/// `BReleaseSteamPipe` for 018 and 019 (experiment 24, F): lowest result
/// byte 1 for each open pipe. Releasing a pipe that is not open was never
/// recorded.
extern "C" fn release_steam_pipe_two(_this: *const Object, pipe: i32) -> bool {
    let mut pipes = open_pipes();
    let Some(index) = pipes.iter().position(|&open| open == pipe) else {
        drop(pipes);
        not_implemented(&format!(
            "SteamClient018/019 position 1 for pipe {pipe}, which is not open"
        ));
    };
    pipes.remove(index);
    true
}

/// `BShutdownIfAllPipesClosed` for 018 and 019 (experiment 24, F): false
/// while a pipe is still open, true once all are released.
extern "C" fn shutdown_if_all_pipes_closed_two(_this: *const Object) -> bool {
    open_pipes().is_empty()
}

/// Getters on 018 and 019 answer only on the first pipe; see
/// `create_local_user`.
fn get_on_first_pipe(pipe: i32, version: Option<&str>) -> *mut c_void {
    if pipe != PIPE {
        not_implemented(&format!(
            "SteamClient018/019 getter for {} on pipe {pipe} (no objects for the game server pipe)",
            version.unwrap_or("?")
        ));
    }
    interface_or_stop(version)
}

/// `GetISteamUtils` (pipe, version text) for 018 and 019 (experiment 24).
extern "C" fn get_utils_interface_two(
    _this: *const Object,
    pipe: i32,
    version: *const c_char,
) -> *mut c_void {
    // SAFETY: games pass a NUL-terminated version text here
    // (`docs/shim/objects.md`).
    get_on_first_pipe(pipe, unsafe { text_arg(version) })
}

/// `GetISteamGenericInterface` and the per-interface `GetISteam…` (user,
/// pipe, version text) for 018 and 019 (experiment 24).
extern "C" fn get_user_interface_two(
    _this: *const Object,
    _user: i32,
    pipe: i32,
    version: *const c_char,
) -> *mut c_void {
    // SAFETY: as above.
    get_on_first_pipe(pipe, unsafe { text_arg(version) })
}

/// The confirmed positions of 018 and 019 (experiment 24): 0, 2, 3, 9,
/// 12, the per-interface getters 5, 6 and 26 (26 recorded only in 019,
/// confirmed for both by the plan), and the calls at close 4, 1, 23.
/// Positions 7, 34 and 38 stay stops (narrowed, candidate, objects).
fn client_two_pipes(mut table: [usize; SLOTS]) -> [usize; SLOTS] {
    table[0] = create_steam_pipe_two as *const () as usize;
    table[1] = release_steam_pipe_two as *const () as usize;
    table[2] = connect_to_global_user as *const () as usize;
    table[3] = create_local_user as *const () as usize;
    table[4] = release_user as *const () as usize;
    table[9] = get_utils_interface_two as *const () as usize;
    for position in [5, 6, 12, 26] {
        table[position] = get_user_interface_two as *const () as usize;
    }
    table[23] = shutdown_if_all_pipes_closed_two as *const () as usize;
    table
}

fn client_018() -> &'static Built {
    static OBJECT: OnceLock<Built> = OnceLock::new();
    OBJECT.get_or_init(|| Built::new(client_two_pipes(stubs!("SteamClient018"))))
}

fn client_019() -> &'static Built {
    static OBJECT: OnceLock<Built> = OnceLock::new();
    OBJECT.get_or_init(|| Built::new(client_two_pipes(stubs!("SteamClient019"))))
}

// ---- SteamUtils009 and SteamUtils010 (meanings.md, confirmed) ----

/// `GetAppID`, confirmed (experiments 3 to 5 for 010, 15 for 009): the
/// app `rebornd` started this process for.
extern "C" fn get_app_id(_this: *const Object) -> u32 {
    match session::get() {
        Some(session) => session.app_id,
        None => not_implemented("SteamUtils position 9 without a session"),
    }
}

/// `GetCurrentBatteryPower`, confirmed (experiment 10 for 010, 15 for
/// 009): 255 on AC power,
/// otherwise the battery level in percent (`crate::power`).
extern "C" fn get_current_battery_power(_this: *const Object) -> u8 {
    crate::power::battery_power()
}

/// Positions 9 and 14, the same in 009 and 010 (experiments 3, 10, 15).
fn add_utils(table: &mut [usize; SLOTS]) {
    table[9] = get_app_id as *const () as usize;
    table[14] = get_current_battery_power as *const () as usize;
}

/// `SetWarningMessageHook`, `SteamUtils009` position 16, confirmed
/// "(experiment 31, under A1 and A2)" (`meanings.md`, "`SteamUtils009` 16:
/// candidate `SetWarningMessageHook`" and experiment 31): the game passes
/// one of its own functions in `rdx` (an executable address outside
/// `steamclient64`, U1 and U2) and reads no result (`rax` was leftover at
/// every call, U3). The meaning is from Valve's public `ISteamUtils` page
/// (<https://partner.steamgames.com/doc/api/ISteamUtils>), as read for
/// that experiment: a function pointer, void (the table in `meanings.md`).
///
/// Reborn's choice, not a recording: the hook is accepted and **never
/// called**, and nothing is stored. The shim has no warning messages of
/// its own to pass on, and calling into a game function whose arguments
/// no recording shows would be a guess. A later hook replacing an earlier
/// one (Dark Deception sets it twice on one object) needs nothing either.
/// The address is never read or logged. No shadow-mode entry: there is no
/// result to compare (LOOP.md step 7 asks for one only where Reborn
/// computes an answer). Only 009: `SteamUtils010` position 16 was never
/// recorded, so it stays a stop.
extern "C" fn set_warning_message_hook(_this: *const Object, _hook: *const c_void) {}

fn utils_009() -> &'static Built {
    static OBJECT: OnceLock<Built> = OnceLock::new();
    OBJECT.get_or_init(|| {
        let mut table = stubs!("SteamUtils009");
        add_utils(&mut table);
        table[16] = set_warning_message_hook as *const () as usize;
        Built::new(table)
    })
}

fn utils_010() -> &'static Built {
    static OBJECT: OnceLock<Built> = OnceLock::new();
    OBJECT.get_or_init(|| {
        let mut table = stubs!("SteamUtils010");
        add_utils(&mut table);
        Built::new(table)
    })
}

// ---- STEAMAPPS_INTERFACE_VERSION008 (meanings.md, experiment 8) ----

/// What the shim answers for `BIsSubscribed`, given `rebornd`'s response
/// to `AppOwnership` for the session's app: the lowest byte of the result,
/// 1 or 0 (experiment 8 recorded 1 for owned games), or `None` when there
/// is no real answer. `Unavailable` (any reason), any other response and a
/// failed request are never turned into an answer (ADR 0055, "Honest
/// failure"): no recorded behaviour says what the official client does
/// then, so the caller stops the game.
pub(crate) fn subscribed_answer(
    response: &Result<reborn_shim_protocol::Response, session::RequestError>,
) -> Option<u8> {
    match response {
        Ok(reborn_shim_protocol::Response::Ownership { owned }) => Some(u8::from(*owned)),
        _ => None,
    }
}

/// `BIsSubscribed`, position 0, confirmed (experiment 8, maintainer OK
/// 2026-11-22). Answer mode in the shim is allowed by the maintainer for
/// this call, for the game itself only, after shadow mode matched in three
/// games (`meanings.md`, "Shadow mode with real account data:
/// `BIsSubscribed`"). The answer is `rebornd`'s `AppOwnership` for the
/// session's own app, from the account's licence list (ADR 0055). `rdx`
/// and `r8` held a 1 and a value that is not an address in the recording;
/// the function takes no arguments, so they are not read. The result is a
/// `u8`: the game reads the lowest byte (experiment 8, prediction 2).
/// Whether the app is owned is account data: it is not logged.
extern "C" fn is_subscribed(_this: *const Object) -> u8 {
    let Some(app_id) = session::get().map(|session| session.app_id) else {
        not_implemented("STEAMAPPS_INTERFACE_VERSION008 position 0 without a session");
    };
    let response = session::request(reborn_shim_protocol::Request::AppOwnership { app_id });
    match subscribed_answer(&response) {
        Some(answer) => answer,
        None => {
            let why = match &response {
                Ok(reborn_shim_protocol::Response::Unavailable { reason }) => {
                    format!("rebornd has no answer ({reason:?})")
                }
                Ok(_) => "rebornd answered something else".to_owned(),
                Err(error) => error.to_string(),
            };
            not_implemented(&format!(
                "STEAMAPPS_INTERFACE_VERSION008 position 0 (BIsSubscribed): {why}"
            ))
        }
    }
}

/// Only position 0 is answered. Positions 7 (`BIsDlcInstalled` or
/// `BIsSubscribedApp`), 6 and 4 are not confirmed as one meaning or not
/// shadowed yet (ADR 0055 stage 2), and stay stubs that stop the game.
fn apps_008() -> &'static Built {
    static OBJECT: OnceLock<Built> = OnceLock::new();
    OBJECT.get_or_init(|| {
        let mut table = stubs!("STEAMAPPS_INTERFACE_VERSION008");
        table[0] = is_subscribed as *const () as usize;
        Built::new(table)
    })
}

/// The version texts the shim has an object for.
pub fn known_interfaces() -> &'static [&'static str] {
    &[
        "SteamClient018",
        "SteamClient019",
        "SteamClient020",
        "SteamClient021",
        "SteamClient023",
        "SteamUtils009",
        "SteamUtils010",
        "STEAMAPPS_INTERFACE_VERSION008",
    ]
}

/// The shim's object for a version text, if it has one.
pub fn interface_for(version: &str) -> Option<*mut c_void> {
    built_for(version).map(Built::pointer)
}

fn built_for(version: &str) -> Option<&'static Built> {
    let built = match version {
        "SteamClient018" => client_018(),
        "SteamClient019" => client_019(),
        "SteamClient020" => client_020(),
        "SteamClient021" => client_021(),
        "SteamClient023" => client_023(),
        "SteamUtils009" => utils_009(),
        "SteamUtils010" => utils_010(),
        "STEAMAPPS_INTERFACE_VERSION008" => apps_008(),
        _ => return None,
    };
    Some(built)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reborn_shim_protocol::{PROTOCOL_VERSION, Session};

    /// The function at `position` of the table of the object for
    /// `version`, read without going through raw pointers.
    fn slot_of(version: &str, position: usize) -> usize {
        built_for(version).map_or(0, |built| built.table[position])
    }

    fn object(version: &str) -> *mut c_void {
        interface_for(version).unwrap_or(std::ptr::null_mut())
    }

    #[test]
    fn only_known_texts_get_objects() {
        for version in known_interfaces() {
            assert!(interface_for(version).is_some(), "{version}");
        }
        assert!(interface_for("SteamClient017").is_none());
        assert!(interface_for("SteamUtils008").is_none());
        assert!(interface_for("SteamUser023").is_none());
        assert!(
            interface_for("STEAMAPPS_INTERFACE_VERSION009").is_none(),
            "009 (asked by the SteamClient023 game) has no confirmed meaning"
        );
        assert_eq!(
            object("SteamClient020"),
            object("SteamClient020"),
            "one object per text"
        );
    }

    #[test]
    fn steam_client_answers_pipe_user_and_utils() {
        for version in ["SteamClient020", "SteamClient021", "SteamClient023"] {
            let client = object(version);
            // SAFETY: positions 0, 2 and 9 hold the functions with these
            // signatures (`client_table`).
            unsafe {
                let pipe: extern "C" fn(*const Object) -> i32 =
                    std::mem::transmute(slot_of(version, 0));
                let user: extern "C" fn(*const Object, i32) -> i32 =
                    std::mem::transmute(slot_of(version, 2));
                let utils: extern "C" fn(*const Object, i32, *const c_char) -> *mut c_void =
                    std::mem::transmute(slot_of(version, 9));
                let generic: extern "C" fn(*const Object, i32, i32, *const c_char) -> *mut c_void =
                    std::mem::transmute(slot_of(version, 12));
                let p = pipe(client.cast());
                let u = user(client.cast(), p);
                assert_eq!((p, u), (1, 1), "{version}");
                let text = c"SteamUtils010";
                assert_eq!(
                    utils(client.cast(), p, text.as_ptr()),
                    object("SteamUtils010")
                );
                assert_eq!(
                    generic(client.cast(), u, p, text.as_ptr()),
                    object("SteamUtils010")
                );
            }
        }
    }

    #[test]
    fn per_interface_positions_differ_by_version() {
        let generic = get_user_interface as *const () as usize;
        assert_eq!(
            slot_of("SteamClient020", 30),
            generic,
            "MUSICREMOTE at 30 in 020"
        );
        assert_ne!(
            slot_of("SteamClient023", 30),
            generic,
            "30 is not an interface in 023"
        );
        assert_eq!(slot_of("SteamClient023", 23), generic, "HTTP at 23 in 023");
        assert_ne!(
            slot_of("SteamClient020", 23),
            generic,
            "23 is the shutdown in 020"
        );
        assert_eq!(
            slot_of("SteamClient020", 23),
            shutdown_if_all_pipes_closed as *const () as usize
        );
        let shutdown = shutdown_if_all_pipes_closed as *const () as usize;
        assert_eq!(
            slot_of("SteamClient021", 23),
            shutdown,
            "021 shuts down at 23"
        );
        assert_eq!(
            slot_of("SteamClient023", 22),
            shutdown,
            "023 shuts down at 22"
        );
        for version in ["SteamClient020", "SteamClient021", "SteamClient023"] {
            assert_eq!(
                slot_of(version, 4),
                release_user as *const () as usize,
                "{version}"
            );
            assert_eq!(
                slot_of(version, 1),
                release_steam_pipe as *const () as usize,
                "{version}"
            );
        }
    }

    #[test]
    fn two_pipe_clients_have_only_the_confirmed_positions() {
        let implemented = [
            (0, create_steam_pipe_two as *const () as usize),
            (1, release_steam_pipe_two as *const () as usize),
            (2, connect_to_global_user as *const () as usize),
            (3, create_local_user as *const () as usize),
            (4, release_user as *const () as usize),
            (5, get_user_interface_two as *const () as usize),
            (6, get_user_interface_two as *const () as usize),
            (9, get_utils_interface_two as *const () as usize),
            (12, get_user_interface_two as *const () as usize),
            (23, shutdown_if_all_pipes_closed_two as *const () as usize),
            (26, get_user_interface_two as *const () as usize),
        ];
        for version in ["SteamClient018", "SteamClient019"] {
            for (position, function) in implemented {
                assert_eq!(slot_of(version, position), function, "{version} {position}");
            }
            for position in (0..SLOTS).filter(|p| !implemented.iter().any(|(i, _)| i == p)) {
                assert!(
                    !implemented
                        .iter()
                        .any(|&(_, function)| function == slot_of(version, position)),
                    "{version} {position} must stay a stop"
                );
            }
        }
        assert_ne!(object("SteamClient018"), object("SteamClient019"));
    }

    /// The whole recorded sequence of experiment 24 on both versions:
    /// Gravitas (018) releases pipe 2 first, Dark Deception (019) pipe 1
    /// first. The only test that uses the pipe list.
    #[test]
    fn two_pipe_clients_open_and_close_as_recorded() {
        for (version, first, second) in [("SteamClient018", 2, 1), ("SteamClient019", 1, 2)] {
            let client = object(version).cast::<Object>().cast_const();
            // SAFETY: the positions hold the functions with these
            // signatures (`client_two_pipes`); `q` is a writable `i32`.
            unsafe {
                let pipe: extern "C" fn(*const Object) -> i32 =
                    std::mem::transmute(slot_of(version, 0));
                let user: extern "C" fn(*const Object, i32) -> i32 =
                    std::mem::transmute(slot_of(version, 2));
                let local_user: extern "C" fn(*const Object, *mut i32, i32) -> i32 =
                    std::mem::transmute(slot_of(version, 3));
                let utils: extern "C" fn(*const Object, i32, *const c_char) -> *mut c_void =
                    std::mem::transmute(slot_of(version, 9));
                let generic: extern "C" fn(*const Object, i32, i32, *const c_char) -> *mut c_void =
                    std::mem::transmute(slot_of(version, 12));
                let release_user: extern "C" fn(*const Object, i32, i32) =
                    std::mem::transmute(slot_of(version, 4));
                let release_pipe: extern "C" fn(*const Object, i32) -> bool =
                    std::mem::transmute(slot_of(version, 1));
                let shutdown: extern "C" fn(*const Object) -> bool =
                    std::mem::transmute(slot_of(version, 23));

                let p = pipe(client);
                let u = user(client, p);
                assert_eq!((p, u), (1, 1), "{version}");
                let text = c"SteamUtils009";
                assert_eq!(utils(client, p, text.as_ptr()), object("SteamUtils009"));
                assert_eq!(
                    generic(client, 0, p, text.as_ptr()),
                    object("SteamUtils009"),
                    "user 0, as recorded"
                );

                let mut q = 0i32;
                let v = local_user(client, &mut q, 3);
                assert_eq!((q, v), (2, 1), "{version}");

                for (pipe, last) in [(first, false), (second, true)] {
                    release_user(client, pipe, 1);
                    assert!(release_pipe(client, pipe), "{version} pipe {pipe}");
                    assert_eq!(shutdown(client), last, "{version} after pipe {pipe}");
                }
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn battery_power_is_a_percentage_or_255() {
        for version in ["SteamUtils009", "SteamUtils010"] {
            // SAFETY: position 14 holds `get_current_battery_power`.
            let power: extern "C" fn(*const Object) -> u8 =
                unsafe { std::mem::transmute(slot_of(version, 14)) };
            let value = power(object(version).cast());
            assert!(value == 255 || value <= 100, "{version}: {value}");
        }
    }

    #[test]
    fn utils_009_accepts_the_warning_hook_and_never_calls_it() {
        static CALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        extern "C" fn hook(_severity: i32, _text: *const c_char) {
            CALLED.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        assert_eq!(
            slot_of("SteamUtils009", 16),
            set_warning_message_hook as *const () as usize
        );
        // SAFETY: position 16 of 009 holds `set_warning_message_hook`,
        // with this signature.
        let set: extern "C" fn(*const Object, *const c_void) =
            unsafe { std::mem::transmute(slot_of("SteamUtils009", 16)) };
        let utils = object("SteamUtils009").cast::<Object>().cast_const();
        // Twice, as Dark Deception does, then a null hook.
        set(utils, hook as *const () as *const c_void);
        set(utils, hook as *const () as *const c_void);
        set(utils, std::ptr::null());
        assert!(!CALLED.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn warning_hook_is_only_at_009_position_16() {
        let hook = set_warning_message_hook as *const () as usize;
        for position in 0..SLOTS {
            assert_ne!(slot_of("SteamUtils010", position), hook, "010 {position}");
        }
        for position in (0..SLOTS).filter(|&p| p != 16) {
            assert_ne!(slot_of("SteamUtils009", position), hook, "009 {position}");
        }
    }

    #[test]
    fn get_app_id_answers_from_the_session() {
        session::set_for_test(Session {
            version: PROTOCOL_VERSION,
            app_id: 2_143_030,
        });
        for version in ["SteamUtils009", "SteamUtils010"] {
            let utils = object(version);
            // SAFETY: position 9 holds `get_app_id`.
            let app_id: extern "C" fn(*const Object) -> u32 =
                unsafe { std::mem::transmute(slot_of(version, 9)) };
            assert_eq!(app_id(utils.cast()), 2_143_030, "{version}");
        }
    }

    #[test]
    fn subscribed_maps_ownership_to_1_or_0_and_nothing_else() {
        use reborn_shim_protocol::{Response, UnavailableReason};
        assert_eq!(
            subscribed_answer(&Ok(Response::Ownership { owned: true })),
            Some(1)
        );
        assert_eq!(
            subscribed_answer(&Ok(Response::Ownership { owned: false })),
            Some(0)
        );
        for reason in [
            UnavailableReason::NotSignedIn,
            UnavailableReason::Offline,
            UnavailableReason::NotAllowed,
            UnavailableReason::TooLarge,
            UnavailableReason::NotReceived,
        ] {
            assert_eq!(
                subscribed_answer(&Ok(Response::Unavailable { reason })),
                None,
                "{reason:?}"
            );
        }
        assert_eq!(subscribed_answer(&Ok(Response::Accepted)), None);
        assert_eq!(
            subscribed_answer(&Err(session::RequestError::OutOfStep("test"))),
            None
        );
        assert_eq!(
            subscribed_answer(&Err(session::RequestError::TimedOut)),
            None
        );
        assert_eq!(
            subscribed_answer(&Err(session::RequestError::NoSession)),
            None
        );
    }

    #[test]
    fn apps_008_answers_only_position_0() {
        let subscribed = is_subscribed as *const () as usize;
        assert_eq!(slot_of("STEAMAPPS_INTERFACE_VERSION008", 0), subscribed);
        for position in 1..SLOTS {
            let slot = slot_of("STEAMAPPS_INTERFACE_VERSION008", position);
            assert_ne!(slot, subscribed, "position {position}");
            assert_ne!(slot, 0, "position {position}");
        }
    }

    #[test]
    fn is_subscribed_asks_rebornd_for_the_session_app() {
        use crate::session::fake::{Fake, Reply};
        use reborn_shim_protocol::{Request, Response};
        // The same app as `get_app_id_answers_from_the_session`: the
        // session is set once per test process.
        session::set_for_test(Session {
            version: PROTOCOL_VERSION,
            app_id: 2_143_030,
        });
        let fake = Fake::new(|message| match message.request {
            Request::AppOwnership { app_id: 2_143_030 } => {
                Some(Reply::Response(Response::Ownership { owned: true }))
            }
            Request::Hello { .. } => None,
            _ => Some(Reply::Broken),
        });
        let mut link = session::Link::new(fake.opener(), std::time::Duration::from_secs(30));
        assert!(matches!(link.start(), Ok(Some(_))));
        session::set_link_for_test(link);
        // SAFETY: position 0 holds `is_subscribed`, with this signature.
        let subscribed: extern "C" fn(*const Object) -> u8 =
            unsafe { std::mem::transmute(slot_of("STEAMAPPS_INTERFACE_VERSION008", 0)) };
        let apps = object("STEAMAPPS_INTERFACE_VERSION008");
        assert_eq!(subscribed(apps.cast()), 1);
        assert_eq!(
            fake.requests(),
            [[
                "{\"id\":2,\"request\":{\"type\":\"app_ownership\",\"app_id\":2143030}}
"
            ]]
        );
    }
}
