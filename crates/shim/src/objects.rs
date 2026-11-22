//! The objects the shim hands out: a pointer to a table of functions, as
//! the official client's objects are shaped (`docs/shim/objects.md`, step
//! 3a). Positions not listed here end the process with a log line.

use std::ffi::{c_char, c_void};
use std::sync::OnceLock;

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
const USER: i32 = 1;

/// `CreateSteamPipe` candidate, matched in experiment 7.
extern "C" fn create_steam_pipe(_this: *const Object) -> i32 {
    PIPE
}

/// `ConnectToGlobalUser` candidate, matched in experiment 7.
extern "C" fn connect_to_global_user(_this: *const Object, _pipe: i32) -> i32 {
    USER
}

/// `GetISteamUtils` candidate (pipe, version text), matched in experiment 7.
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
/// candidates (user, pipe, version text), matched in experiment 7.
extern "C" fn get_user_interface(
    _this: *const Object,
    _user: i32,
    _pipe: i32,
    version: *const c_char,
) -> *mut c_void {
    // SAFETY: as above.
    interface_or_stop(unsafe { text_arg(version) })
}

/// `ReleaseUser` candidate (020 position 4), matched in experiment 7.
extern "C" fn release_user(_this: *const Object, _pipe: i32, _user: i32) {}

/// `BReleaseSteamPipe` candidate (020 position 1), matched in experiment 7.
extern "C" fn release_steam_pipe(_this: *const Object, _pipe: i32) -> bool {
    true
}

/// `BShutdownIfAllPipesClosed` candidate (020 position 23), matched in
/// experiment 7.
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

fn client_020() -> &'static Built {
    static OBJECT: OnceLock<Built> = OnceLock::new();
    OBJECT.get_or_init(|| {
        let mut table = client_table(stubs!("SteamClient020"), USER_INTERFACE_POSITIONS_020);
        table[1] = release_steam_pipe as *const () as usize;
        table[4] = release_user as *const () as usize;
        table[23] = shutdown_if_all_pipes_closed as *const () as usize;
        Built::new(table)
    })
}

fn client_021() -> &'static Built {
    static OBJECT: OnceLock<Built> = OnceLock::new();
    OBJECT.get_or_init(|| {
        Built::new(client_table(
            stubs!("SteamClient021"),
            USER_INTERFACE_POSITIONS_021,
        ))
    })
}

fn client_023() -> &'static Built {
    static OBJECT: OnceLock<Built> = OnceLock::new();
    OBJECT.get_or_init(|| {
        Built::new(client_table(
            stubs!("SteamClient023"),
            USER_INTERFACE_POSITIONS_023,
        ))
    })
}

// ---- SteamUtils010 (meanings.md, confirmed) ----

/// `GetAppID`, confirmed (experiments 3 to 5): the app `rebornd` started
/// this process for.
extern "C" fn get_app_id(_this: *const Object) -> u32 {
    match session::get() {
        Some(session) => session.app_id,
        None => not_implemented("SteamUtils010 position 9 without a session"),
    }
}

fn utils_010() -> &'static Built {
    static OBJECT: OnceLock<Built> = OnceLock::new();
    OBJECT.get_or_init(|| {
        let mut table = stubs!("SteamUtils010");
        table[9] = get_app_id as *const () as usize;
        Built::new(table)
    })
}

/// The version texts the shim has an object for.
pub fn known_interfaces() -> &'static [&'static str] {
    &[
        "SteamClient020",
        "SteamClient021",
        "SteamClient023",
        "SteamUtils010",
    ]
}

/// The shim's object for a version text, if it has one.
pub fn interface_for(version: &str) -> Option<*mut c_void> {
    built_for(version).map(Built::pointer)
}

fn built_for(version: &str) -> Option<&'static Built> {
    let built = match version {
        "SteamClient020" => client_020(),
        "SteamClient021" => client_021(),
        "SteamClient023" => client_023(),
        "SteamUtils010" => utils_010(),
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
        assert!(interface_for("SteamUtils009").is_none());
        assert!(interface_for("SteamUser023").is_none());
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
        assert_ne!(
            slot_of("SteamClient021", 23),
            shutdown_if_all_pipes_closed as *const () as usize,
            "the close calls are recorded for 020 only"
        );
    }

    #[test]
    fn get_app_id_answers_from_the_session() {
        session::set_for_test(Session {
            version: PROTOCOL_VERSION,
            app_id: 2_143_030,
        });
        let utils = object("SteamUtils010");
        // SAFETY: position 9 holds `get_app_id`.
        let app_id: extern "C" fn(*const Object) -> u32 =
            unsafe { std::mem::transmute(slot_of("SteamUtils010", 9)) };
        assert_eq!(app_id(utils.cast()), 2_143_030);
    }
}
