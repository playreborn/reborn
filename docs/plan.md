# Reborn – Projektplan

**Name:** Reborn (siehe [3.4](#34-name))
**Repository:** `github.com/playreborn/reborn`
**crates.io:** `reborn`
**Projektart:** Open-Source-Reimplementierung eines Steam-kompatiblen Desktop-Clients
**Sprache:** Rust
**GUI:** Iced
**Plattformen:** Windows 10/11, Linux, SteamOS (Desktop-Modus); macOS später
**Lizenz:** GPL-3.0-or-later (Anwendungen), MIT OR Apache-2.0 (Protokoll- und Format-Crates)
**Status:** Planung
**Dokumentversion:** 2.0 (2026-11-19), ersetzt v1

---

## 0. Kurzfassung

- Reborn ist ein eigenständiger Client für das Steam-Netzwerk, von Grund auf neu in Rust geschrieben. Er nutzt deinen echten Steam-Account, deine echten Lizenzen und Valves echte Server. Er ersetzt den **Client**, nicht die Plattform.
- Das Endziel: Der originale Steam-Client muss nicht mehr installiert sein, auch nicht für Spiele, die Steamworks verwenden.
- Das Projekt besteht aus zwei Teilen:
  - **Teil A (Client ↔ Valve):** Login, Bibliothek, Downloads, Freunde, Cloud. Machbar, durch bestehende Projekte belegt.
  - **Teil B (Client ↔ Spiel):** ein eigener Ersatz für `steamclient`, damit Steamworks-Spiele ohne Original-Client laufen. Schwer, über Jahre, aber der eigentliche Kern der Unabhängigkeit.
- Wir bauen auf **Interoperabilität durch Reverse Engineering**, so wie SteamKit2, DepotDownloader und node-steam-user seit über zehn Jahren. Dafür gelten feste rote Linien (Abschnitt 2).
- Architektur: ein Hintergrunddienst (`rebornd`), dünne Frontends (Iced-GUI, CLI) und später eine Shim-Bibliothek, die in Spielprozesse geladen wird.
- Die Roadmap fängt mit dem Riskantesten an: zuerst Login und ein komplett selbst heruntergeladenes Spiel per CLI, danach die GUI.
- Entwickelt wird überwiegend KI-gestützt („Vibe-Coding“), mit harten Regeln gegen erfundene Protokolldetails (Abschnitt 12), und offen für Community-Beiträge (Abschnitt 13).

---

## 1. Vision

### 1.1 Was ist Reborn?

Ein schneller, schlanker, quelloffener Client für dein Steam-Konto:

- startet in unter einer Sekunde, braucht wenig RAM und keine eingebettete Browser-Engine,
- macht genau die Dinge, die man mit einem Spiele-Client täglich tut: Bibliothek, Installieren, Aktualisieren, Spielen, Freunde, Chat,
- und lässt alles weg, was nicht dazugehört.

Reborn verkauft keine Spiele, stellt keine Lizenzen aus und betreibt keine eigenen Accounts. Alle Käufe, Lizenzen und Inhalte bleiben bei Valve.

### 1.2 Vorbilder

| Projekt | Was wir davon lernen |
| --- | --- |
| **legendary / Heroic** | Ein Drittanbieter-Client für Epic ist möglich, nützlich und wird angenommen. |
| **SteamKit2, node-steam-user, ValvePython/steam** | Referenzen für das Steam-CM-Protokoll |
| **DepotDownloader** | Referenz für Depots, Manifeste und CDN-Downloads |
| **Wine / Proton / umu-launcher** | Windows-Spiele auf Linux, Proton außerhalb von Steam |
| **GameNetworkingSockets** (Valve, BSD-3) | Offene Basis für Steam-Networking in Teil B |

### 1.3 Leitprinzipien

1. **Minimal:** Jede Funktion muss ihren Platz verdienen. Im Zweifel weglassen.
2. **Kompatibel:** Reborn arbeitet mit vorhandenen Steam-Bibliotheksordnern und kann neben dem Original-Client existieren.
3. **Ehrlich:** Für jedes Spiel und jede Funktion wird dokumentiert, wie gut sie funktioniert. Wir versprechen nichts, was nicht getestet ist.
4. **Sicher:** Zugangsdaten verlassen nie den OS-Schlüsselbund, landen nie in Logs, Fixtures oder Prompts.
5. **Offen:** Der Daemon hat eine dokumentierte API, sodass die Community eigene Frontends bauen kann (TUI, GTK, Web …).

---

## 2. Rote Linien (nicht verhandelbar)

Diese Regeln trennen Reborn von Cracks und Emulatoren, die Spiele ohne Lizenz lauffähig machen. Sie gelten für jeden Beitrag, auch für KI-generierten Code.

1. **Kein DRM-Bypass.** Kein Entfernen von SteamStub, kein Patchen von Spiel-Binaries, keine Umgehung von Lizenzprüfungen.
2. **Keine Umgehung von Steam Guard.** Nur offizielle Login-Wege: Passwort plus Steam Guard (Mail/App) oder QR-Code.
3. **Keine selbst erzeugten Besitznachweise.** Tickets, App-Ownership und DLC-Status kommen immer vom echten Account über Valves Server. Reborn erfindet nie Besitz.
4. **Keine Manipulation von Achievements oder Statistiken.** Schreibzugriffe passieren nur, wenn das Spiel sie selbst über die API auslöst.
5. **Keine Missbrauchs-Automatisierung.** Kein Idle-Farming, kein Trading-Bot, keine Massenkonten, kein Scraping. Für so etwas gibt es andere Projekte, und es würde die Duldung durch Valve gefährden.
6. **Kein Valve-Code, keine Valve-Assets.** Kein Logo, keine Original-UI-Grafiken, kein „Steam“ im Namen.
7. **Keine Telemetrie ohne Opt-in.**
8. **Server schonen.** Rate-Limits, exponentielles Backoff, keine automatisierten Live-Tests gegen Valve in CI.

---

## 3. Rechtliche und strategische Grundlage

> Dieser Abschnitt ist keine Rechtsberatung. Vor Release 1.0 sollte eine fachkundige Prüfung erfolgen.

### 3.1 Interoperabilität

Das Steam-CM-Protokoll ist nicht öffentlich dokumentiert. Reborn implementiert es auf Basis öffentlich verfügbarer Referenzimplementierungen und eigener Beobachtung des Netzwerkverkehrs **unseres eigenen Clients**.

In der EU stützen § 69d Abs. 3 und § 69e UrhG (Art. 5 Abs. 3 und Art. 6 der Richtlinie 2009/24/EG) das Beobachten von Programmen und die Herstellung von Interoperabilität. Valve duldet Drittanbieter-Clients wie SteamKit-basierte Tools seit vielen Jahren. Das ist eine Duldung, keine Garantie.

### 3.2 Account-Risiko

- Nutzer werden beim ersten Start klar darauf hingewiesen, dass Reborn ein inoffizieller Client ist.
- Entwickler testen mit einem **Zweitaccount**.
- Reborn verhält sich gegenüber den Servern wie ein normaler, höflicher Client (Abschnitt 2, Punkt 8).

### 3.3 Lizenzen

| Teil | Lizenz | Grund |
| --- | --- | --- |
| `rebornd`, GUI, CLI, Kernlogik | GPL-3.0-or-later | Niemand soll eine geschlossene oder manipulierte Version verteilen können. |
| Protokoll- und Format-Crates (`proto`, `formats`, `cm`, `content`) | MIT OR Apache-2.0 | Andere Rust-Projekte sollen sie frei nutzen können. Das stärkt das Ökosystem und bringt Mitwirkende. |
| Shim (`steamclient`-Ersatz) | MIT OR Apache-2.0 | Wird in proprietäre Spielprozesse geladen, keine Lizenzdiskussion nötig. |

**Regeln für fremden Code:**

- Referenzen *lesen* ist immer erlaubt. Code *übernehmen* nur aus kompatibel lizenzierten Quellen (MIT, Apache-2.0, BSD), mit Attribution.
- SteamKit2 (LGPL-2.1) und DepotDownloader (GPL-2.0) dienen nur als Wissensquelle. Kein 1:1-Portieren von Code.
- Protobuf-Definitionen kommen aus dem Community-Projekt SteamDatabase/Protobufs, wie bei allen vergleichbaren Projekten. Herkunft und Risiko werden in einem ADR dokumentiert.
- Beiträge werden per **DCO** (`Signed-off-by`) gezeichnet, kein CLA.

### 3.4 Name

„Steam“ ist eine Marke von Valve. Ein Projektname mit „Steam“ lädt zu einer Takedown-Aufforderung ein und erweckt den Eindruck eines offiziellen Produkts. Deshalb heißt das Projekt **Reborn**.

| Ort | Name |
| --- | --- |
| Projekt, App, Dokumentation | Reborn |
| GitHub-Organisation | `playreborn` (`reborn` war vergeben) |
| Haupt-Repository | `playreborn/reborn` |
| crates.io | `reborn`; weitere Crates mit Präfix: `reborn-proto`, `reborn-formats`, `reborn-cm`, `reborn-content` … |
| Binaries | `reborn` (GUI), `rebornd` (Daemon), `reborn-cli` |

**Auffindbarkeit:** „Reborn“ ist generisch. Überall dort, wo das Projekt vorgestellt wird, steht deshalb dieselbe Unterzeile: *„Reborn – a native, open-source client for your Steam library.“* „Steam“ erscheint dort nur beschreibend, nie im Namen oder Logo.

**crates.io:** Leere Platzhalter-Crates sind dort nicht erlaubt und können entfernt werden. Weitere Namen werden gesichert, indem früh echte, nutzbare Crates veröffentlicht werden. Der erste Kandidat ist `reborn-formats` (VDF/ACF-Parser) aus M0.

README und App tragen den Hinweis: *„Nicht mit Valve Corporation verbunden. Steam ist eine Marke der Valve Corporation.“*

---

## 4. Problemlandkarte

### 4.1 Zwei Teile

```text
                 Teil A                                  Teil B
   ┌──────────────────────────────┐        ┌──────────────────────────────┐
   │  Valve-Server                │        │  Spielprozess                │
   │  (CM, Web-API, CDN)          │        │  steam_api.dll / .so         │
   └──────────────▲───────────────┘        └──────────────┬───────────────┘
                  │ CM-Protokoll, HTTPS                   │ lädt steamclient
   ┌──────────────┴───────────────────────────────────────▼───────────────┐
   │                               rebornd                                │
   │      Auth · Bibliothek · Downloads · Social · Cloud · Shim-Server    │
   └──────────────────────────────────────────────────────────────────────┘
```

- **Teil A** spricht mit Valve. Alles hier ist durch bestehende Open-Source-Projekte belegt machbar.
- **Teil B** spricht mit dem Spiel. Fast jedes Steam-Spiel ruft beim Start `SteamAPI_Init()` auf. Läuft kein Client, bricht das Spiel ab oder startet Steam neu. Ohne Teil B bleibt Reborn ein Launcher, der den Original-Client voraussetzt.

### 4.2 Funktionen im Überblick

| Funktion | Teil | Machbarkeit | Meilenstein |
| --- | --- | --- | --- |
| Login (Passwort + Guard, QR), Sessions | A | Gut belegt | M0 |
| Lizenzen, Bibliothek, App-Infos (PICS) | A | Gut belegt | M0 |
| Download, Update, Verifizieren, Deinstallieren | A | Gut belegt (DepotDownloader) | M1 |
| Spiele ohne Steamworks starten | – | Einfach | M2 |
| Windows-Spiele auf Linux via Proton | A + Plattform | Belegt (umu-launcher) | M2 |
| Freunde, Online-Status, Chat | A | Belegt (node-steam-user) | M3 |
| Achievements anzeigen | A | Belegt | M3 |
| Steamworks-Spiele ohne Original-Client starten | B | Schwer | M4 |
| Achievements freischalten, Statistiken | B | Schwer | M4 |
| Steam Cloud (automatisch beim Start/Ende) | A + B | Mittel, weniger dokumentiert | M4 |
| Lobbys, Matchmaking, Rich Presence | B | Schwer | M5 |
| Steam-Networking (P2P, Relays/SDR) | B | Sehr schwer | M5 |
| Workshop-Inhalte (Abos, Downloads) | A + B | Mittel | M6 |
| Store durchstöbern | A | Mittel (inoffizielle Store-APIs) | M6 |
| Kaufen | – | Nur Weiterleitung in den Browser | M6 |
| Overlay | B | Sehr schwer | später |
| Steam Input (Controller-Remapping) | B | Sehr schwer | später |
| Spiele mit Kernel-Anti-Cheat (EAC/BattlEye) | B | Unklar, pro Spiel | später |

---

## 5. Umfang

### 5.1 Drin (bis 1.0)

Login und mehrere Accounts, Bibliothek mit Sammlungen, Suche und Filtern, Installieren/Aktualisieren/Verifizieren/Verschieben, Download-Warteschlange mit Bandbreitenlimit, Spielstart mit Startoptionen, Proton-Verwaltung auf Linux, Spielzeit, Freunde und Chat, Achievements, Cloud-Sync mit Konfliktauflösung, Workshop-Abos, Store zum Stöbern, Einstellungen.

### 5.2 Später / vielleicht

Overlay, Steam Input, Controller-Oberfläche (Big-Picture-Ersatz), SteamOS-Gaming-Modus, Steam Families/Family Sharing, Screenshots-Verwaltung, macOS.

### 5.3 Nie (Streichliste gegen Bloat)

Community-Hub, Punkte-Shop, Marktplatz und Trading, Broadcasting, Remote Play, VR-Funktionen, Werbung, Empfehlungs-Feeds, eingebettete Browser-Engine.

---

## 6. Architektur

### 6.1 Prozessmodell

```text
   ┌────────────┐   ┌────────────┐   ┌─────────────────┐
   │  reborn    │   │ reborn-cli │   │ Community-      │
   │  (Iced)    │   │            │   │ Frontends       │
   └─────┬──────┘   └─────┬──────┘   └────────┬────────┘
         └────────────────┼───────────────────┘
                          │  lokale IPC (Named Pipe / Unix-Socket)
                 ┌────────▼─────────┐
                 │     rebornd      │──── CM (WebSocket/TLS) ───► Valve
                 │  (Hintergrund-   │──── HTTPS (Web-API, CDN) ─► Valve
                 │   dienst)        │
                 └────────▲─────────┘
                          │  Shim-IPC (ab M4)
                 ┌────────┴─────────┐
                 │  Spielprozess    │
                 │  steam_api ──►   │
                 │  reborn-shim     │
                 └──────────────────┘
```

**Warum ein Daemon?**

- Downloads laufen weiter, wenn das Fenster geschlossen ist.
- Für Teil B braucht es einen langlebigen Prozess mit Steam-Verbindung, an den sich Spiele hängen.
- Stürzt die GUI ab, laufen Downloads und Spiele weiter.
- Andere Frontends lassen sich ohne Eingriff in den Kern bauen.
- Headless-Betrieb (Server, Skripte, SteamOS) ist ohne Zusatzaufwand möglich.

### 6.2 IPC

**Frontend ↔ Daemon**

- Lokaler Socket: Named Pipe auf Windows, Unix-Socket auf Linux.
- Längen-präfixierte Nachrichten, serialisiert mit serde. Format per ADR festlegen; Kandidaten sind JSON (leicht zu debuggen, gut für Fremd-Frontends) und postcard/MessagePack.
- Request/Response plus Event-Stream (Download-Fortschritt, Freundesstatus, Spielstatus).
- Versioniert: Frontends prüfen beim Verbinden die API-Version.
- Zugriff nur für den eigenen Benutzer (Dateirechte bzw. Pipe-ACL).

**Shim ↔ Daemon (ab M4)**

- Eigener, minimaler und latenzarmer Kanal. Spiele fragen Callbacks pro Frame ab (`SteamAPI_RunCallbacks`).
- Die Shim hält möglichst wenig Zustand. Logik und Steam-Verbindung bleiben im Daemon.

### 6.3 Crate-Struktur

Wir starten klein und teilen erst auf, wenn es wehtut.

```text
reborn/
├── Cargo.toml                 # Workspace
├── AGENTS.md                  # Regeln für KI-Agenten (Abschnitt 12)
├── CONTRIBUTING.md
├── SECURITY.md
├── LICENSE-GPL, LICENSE-MIT, LICENSE-APACHE
├── crates/
│   ├── proto/       # generierte Protobufs, EMsg, Enums            (MIT/Apache)
│   ├── formats/     # VDF (Text/Binär), ACF, appinfo, Manifeste      (MIT/Apache)
│   ├── cm/          # CM-Verbindung, Session, Jobs, Service-Calls    (MIT/Apache)
│   ├── content/     # CDN, Depots, Chunks, Verifizierung, Staging    (MIT/Apache)
│   ├── core/        # Domäne: Accounts, Bibliothek, Queue, Launch    (GPL)
│   ├── platform/    # Pfade, Schlüsselbund, Prozesse, Proton         (GPL)
│   ├── ipc/         # API-Typen Frontend ↔ Daemon                    (GPL)
│   └── testkit/     # Fixtures, Replay, Mocks                        (GPL)
├── apps/
│   ├── rebornd/     # Daemon
│   ├── reborn/      # Iced-GUI
│   └── reborn-cli/  # CLI
├── shim/            # ab M4: steamclient-Ersatz (cdylib)             (MIT/Apache)
├── compat/          # Kompatibilitätsdatenbank (TOML pro App)
├── fixtures/        # aufgezeichnete, geschwärzte Protokoll-Mitschnitte
└── docs/
    ├── adr/
    ├── protocol/    # unser Protokollwissen mit Quellenangaben
    └── guides/
```

**Abhängigkeitsregeln:**

- `proto` ← `cm` ← `content` ← `core` ← `rebornd`. Keine Zyklen.
- Frontends hängen nur an `ipc`, nie an `cm` oder `content`.
- `#![forbid(unsafe_code)]` in allen Crates außer `platform` und `shim`.

### 6.4 Datenhaltung

| Daten | Ort | Wahrheit |
| --- | --- | --- |
| Installierte Spiele | `steamapps/appmanifest_<id>.acf`, `libraryfolders.vdf` | Steam-Dateiformat, von Reborn **gelesen und geschrieben** |
| App-/Paket-Infos (PICS) | SQLite-Cache | Valve, Cache ist ersetzbar |
| Eigene Daten (Sammlungen, Favoriten, Spielzeit-Cache) | SQLite | Reborn |
| Einstellungen | TOML | Nutzer |
| Refresh-Tokens, Machine-Auth | OS-Schlüsselbund | Nie anderswo |

Vier Datenklassen werden strikt getrennt: von Steam stammend, lokal erkannt, vom Nutzer angepasst und temporärer Cache. So werden lokale Daten nie fälschlich als aktuelle Steam-Daten behandelt.

Weil Reborn die Steam-eigenen Formate schreibt, erkennt der Original-Client von Reborn installierte Spiele und umgekehrt. Ein Umstieg in beide Richtungen ist jederzeit möglich.

### 6.5 Koexistenz mit dem Original-Client

- Gemeinsame Bibliotheksordner sind erlaubt.
- Läuft der Original-Client, schreibt Reborn keine ACF-Dateien der Bibliotheken, die dieser gerade verwaltet (Erkennung über Prozess und Lockfiles).
- Die Shim-Registrierung (Abschnitt 8) erfolgt nur, wenn der Original-Client nicht läuft, und wird beim Beenden zurückgesetzt.
- Mehrere gleichzeitige Sessions desselben Accounts sind möglich. Es kann aber nur eine Session gleichzeitig ein Spiel „spielen“. Reborn behandelt den Konflikt „wird auf einem anderen Gerät gespielt“ sauber.

### 6.6 Sicherheit

- Tokens nur im OS-Schlüsselbund (Windows Credential Manager, Secret Service). Gibt es keinen sicheren Speicher, wird nichts dauerhaft gespeichert.
- Ein Typ `Secret<T>` mit geschwärzter `Debug`/`Display`-Ausgabe für alle sensiblen Werte. Tokens können nicht versehentlich geloggt werden.
- TLS für alle Verbindungen (rustls).
- **Dateinamen aus Manifesten sind nicht vertrauenswürdig:** Schutz gegen Path Traversal und Symlinks, bevor auch nur ein Byte geschrieben wird.
- Updates transaktional: in ein Staging-Verzeichnis laden, verifizieren und erst dann übernehmen. Ein abgebrochenes Update beschädigt nie die vorhandene Installation.
- Cloud-Konflikte werden nie automatisch überschrieben. Der Nutzer entscheidet.
- `cargo-deny` (Lizenzen, bekannte Sicherheitslücken) in CI; signierte Releases.

### 6.7 Technologie

| Bereich | Wahl | Bemerkung |
| --- | --- | --- |
| Sprache | Rust stable, festgelegte MSRV | `rust-toolchain.toml` |
| Async | tokio | |
| GUI | Iced | Version zu Projektbeginn festlegen |
| HTTP/WebSocket | reqwest, tokio-tungstenite, rustls | |
| Protobuf | prost (+ prost-build) | Code wird **generiert**, nie handgeschrieben |
| Datenbank | rusqlite | Einfacher als SQLx für eine eingebettete Einzelnutzer-DB |
| Konfiguration | serde + toml | |
| Fehler | thiserror (Libraries), anyhow (Apps) | |
| Logging | tracing | |
| Schlüsselbund | keyring | |
| Krypto/Kompression | RustCrypto (AES, SHA-1, RSA), lzma-rs, zstd, zip | für Chunks und Passwort-Verschlüsselung |
| Steam lokal finden | steamlocate (evaluieren) | |
| VDF | eigenes `formats`-Crate oder vorhandenen Parser evaluieren | binäres `appinfo.vdf` hat ein neueres Format mit String-Tabelle |
| CM-Protokoll | eigenes `cm`-Crate; `steam-vent` als Basis oder Referenz evaluieren | Entscheidung in M0 per ADR |

---

## 7. Teil A: Protokollwissen

Eine Übersicht der Bausteine, die wir brauchen. Details stehen später in `docs/protocol/`, jeweils mit Quellenangabe (Datei/Zeile in einer Referenzimplementierung oder ein aufgezeichnetes Fixture).

### 7.1 Verbindung

- CM-Serverliste über die Web-API (Steam Directory).
- **WebSocket-Transport über TLS** zuerst. Moderne Clients nutzen ihn, und er erspart den eigenen RSA/AES-Kanal-Handshake der alten TCP-Verbindung.
- Nachrichtenrahmen, Job-IDs, Heartbeats, Reconnect mit Backoff.
- „Unified Messages“ (Service-Methoden wie `Service.Method#1`) als einheitlicher Weg für neuere Funktionen.

### 7.2 Login

- `IAuthenticationService`: RSA-Schlüssel holen, Passwort verschlüsseln, Session per Passwort oder QR-Code starten, auf Bestätigung pollen und Steam-Guard-Codes übermitteln.
- Ergebnis: Refresh-Token (in den Schlüsselbund) und Access-Token.
- Danach Logon am CM mit dem Token und Erneuerung über die vorgesehenen Methoden.

### 7.3 Lizenzen und Bibliothek

- Lizenzliste → Pakete → PICS-Abfragen für Pakete und Apps (inklusive PICS-Access-Tokens).
- Daraus: besessene Apps, Depots, Branches, Startkonfigurationen, Artwork-Referenzen.

### 7.4 Content

1. Depot-Entschlüsselungsschlüssel anfordern.
2. Manifest-Request-Code anfordern (seit 2022 Pflicht).
3. CDN-Serverliste holen.
4. Manifest herunterladen und parsen (Protobuf, Dateinamen ggf. verschlüsselt).
5. Chunks herunterladen, mit AES entschlüsseln, dekomprimieren (VZip/LZMA, Zstd, Zip) und gegen die Prüfsummen verifizieren.
6. Dateien im Staging zusammensetzen, übernehmen und ACF schreiben.
7. Updates: Manifest-Diff, nur geänderte Chunks laden.

### 7.5 Social, Achievements, Cloud

- Freundesliste, Persona-State, Online-Status und Chat über CM-Nachrichten und Service-Methoden.
- Achievements und Statistiken lesen über CM bzw. Web-API.
- Cloud: Dateilisten, Upload und Download über Service-Methoden. Das ist schlechter dokumentiert als der Rest, deshalb gibt es vor der Implementierung einen eigenen Spike.

### 7.6 Pflege

Valve ändert das Protokoll regelmäßig. Die Protobuf-Definitionen werden über ein Skript aus SteamDatabase/Protobufs aktualisiert, mit einem Diff-Report im PR. Ein wöchentlicher CI-Job meldet neue Versionen.

---

## 8. Teil B: steamclient-Ersatz (Shim)

### 8.1 Wie Spiele den Client finden

- Spiele enthalten `steam_api(64).dll` bzw. `libsteam_api.so`. Diese Bibliothek lädt `steamclient` und holt sich darüber versionierte Interfaces (`CreateInterface`, z.B. „SteamClient0XX“, „SteamUser0XX“).
- **Windows:** Der Pfad steht unter `HKCU\Software\Valve\Steam\ActiveProcess` (`SteamClientDll`, `SteamClientDll64`, `pid`).
- **Linux:** `~/.steam/sdk32/steamclient.so` und `~/.steam/sdk64/steamclient.so`.
- **Proton:** Spiele unter Proton sprechen über Protons `lsteamclient` mit dem **nativen Linux-`steamclient.so`**. Eine Linux-Shim würde damit native und Proton-Spiele zugleich bedienen. (In M4 verifizieren.)
- `SteamAPI_RestartAppIfNecessary` startet Steam über `steam://`-URLs. Reborn registriert sich als Handler dafür.

### 8.2 Technik

- Rust-`cdylib`, die `CreateInterface` exportiert und C++-vtables für jede unterstützte Interface-Version nachbildet (`#[repr(C)]`).
- 32-Bit-Windows-Spiele brauchen `extern "thiscall"`. Wir starten mit 64-Bit und fügen 32-Bit später hinzu.
- Mehrere Interface-Versionen gleichzeitig unterstützen. Ältere Spiele fragen ältere Versionen an.
- Die Shim leitet Aufrufe an `rebornd` weiter. Tickets und Besitz kommen immer von Valve (rote Linie 3).

### 8.3 Ausbaustufen

| Stufe | Interfaces | Ergebnis |
| --- | --- | --- |
| B1 Kern | ISteamClient, ISteamUser (Auth-Session-Tickets, Encrypted App Tickets), ISteamApps (Besitz, DLC, Sprache), ISteamUtils, ISteamUserStats, ISteamRemoteStorage | Singleplayer-Spiele mit Achievements und Cloud laufen ohne Original-Client. |
| B2 Social | ISteamFriends, Rich Presence, ISteamMatchmaking (Lobbys) | Freundesanzeige im Spiel, Lobbys, Einladungen |
| B3 Networking | ISteamNetworkingSockets, ISteamNetworkingMessages, ISteamNetworking (alt), Relays/SDR | Multiplayer über Steam-Networking, aufbauend auf GameNetworkingSockets |
| B4 Rest | ISteamUGC (Workshop), ISteamInput, ISteamHTMLSurface usw. | nach Bedarf, getrieben durch die Kompatibilitätsdatenbank |

### 8.4 Testgeschirr

- Ein eigenes **Testspiel** in Rust (z.B. mit `steamworks-rs` gegen Valves öffentliche Test-App „Spacewar“, App-ID 480) ruft jede unterstützte API auf.
- Es läuft einmal gegen den Original-Client (Referenzverhalten aufzeichnen) und einmal gegen die Shim (vergleichen).
- So lässt sich jedes Interface isoliert testen, und die Community kann neue Testfälle beitragen.

### 8.5 Kompatibilitätsstufen

| Stufe | Bedeutung |
| --- | --- |
| 0 | Startet nicht |
| 1 | Startet, nur mit Original-Client als Fallback |
| 2 | Startet ohne Original-Client, Singleplayer spielbar |
| 3 | zusätzlich Achievements und Cloud |
| 4 | zusätzlich Multiplayer |
| 5 | Keine bekannten Unterschiede zum Original-Client |

Die Daten liegen als TOML pro App in `compat/` und werden über Compatibility-Report-Issues gepflegt. Das ist auch ein Weg, ohne Programmierkenntnisse beizutragen.

---

## 9. Plattformen

### 9.1 Windows

- Standard-Bibliothek in einem benutzerbeschreibbaren Ordner, damit kein Admin-Dienst nötig ist.
- Prozessüberwachung über Job Objects (fängt auch Kindprozesse und Launcher).
- Registrierung als `steam://`-Handler nur auf ausdrücklichen Wunsch des Nutzers.

### 9.2 Linux

- **Proton außerhalb von Steam**, mit umu-launcher als Referenz bzw. möglicher Abhängigkeit (inklusive Protonfixes).
- Proton und die Steam Linux Runtime sind selbst Steam-Apps. Reborn lädt sie über die eigene Content-Engine.
- Native Linux-Spiele laufen in der Steam Linux Runtime (pressure-vessel).
- XDG-Pfade, Secret Service, Desktop-Benachrichtigungen.
- Flatpak als bevorzugtes Paketformat.

### 9.3 SteamOS

- Zuerst nur der Desktop-Modus.
- Der Gaming-Modus erwartet Steam als Shell der gamescope-Session. Ein Ersatz dafür ist ein eigenes Großprojekt (später, siehe 5.2).

### 9.4 macOS

Später, gerne als Community-Initiative. Die Architektur verhindert es nicht.

---

## 10. GUI

### 10.1 Grundsätze

- Die GUI ist **dünn**: Sie zeigt Zustand an und schickt Befehle an `rebornd`. Kein Netzwerk, keine Datenbank, keine Geschäftslogik in der GUI.
- Eine eigene visuelle Identität. Reborn soll nicht wie Steam aussehen.
- Tastaturbedienbar von Anfang an. Controller-Navigation später.

### 10.2 Bildschirme bis 1.0

| Bildschirm | Inhalt |
| --- | --- |
| Login | Passwort + Guard, QR-Code, Account-Wechsel |
| Bibliothek | Raster/Liste, Suche, Filter, Sammlungen, installiert/nicht installiert |
| Spieldetails | Installieren, Spielen, Updaten, Verifizieren, Verschieben, Deinstallieren; Startoptionen, Proton-Version, Kompatibilitätsstufe |
| Downloads | Warteschlange, Fortschritt, Geschwindigkeit, ETA, Pausieren, Bandbreitenlimit |
| Freunde und Chat | ab M3 |
| Store | ab M6, nur zum Stöbern; Kauf im Browser |
| Einstellungen | Allgemein, Bibliotheken, Downloads, Accounts, Darstellung, Datenschutz, Erweitert |

### 10.3 Bekannte Iced-Risiken

- **Barrierefreiheit:** Screenreader-Unterstützung (AccessKit) ist noch nicht ausgereift. Den Stand regelmäßig prüfen.
- **IME:** Eingabe in CJK-Sprachen im Chat muss früh getestet werden.
- **Rich Text:** Spielbeschreibungen enthalten HTML bzw. BBCode. Dafür braucht es einen kleinen, sicheren Renderer für eine Teilmenge.
- **Bilder:** Große Cover-Raster brauchen Caching und Lazy-Loading.

Weil die GUI dünn ist, bleibt ein späterer Frameworkwechsel überschaubar.

---

## 11. Roadmap

Grundsatz: **das Riskanteste zuerst.** Jeder Meilenstein endet mit etwas, das man vorführen kann.

### M0 – Fundament und Protokoll-Spike (nur CLI)

- Repository, Workspace, CI, Lizenzen, `AGENTS.md`, Protobuf-Pipeline.
- ADR: eigenes `cm`-Crate oder `steam-vent` als Basis.
- WebSocket-CM-Verbindung, Login per QR und per Passwort + Guard, Refresh-Token im Schlüsselbund.
- Lizenzliste und PICS.

**Fertig, wenn:** `reborn-cli login` und `reborn-cli library` die eigenen Spiele korrekt auflisten und ein Neustart ohne erneuten Login funktioniert.

### M1 – Content-Engine

- Manifeste, Chunks, Entschlüsselung, Dekompression, Verifizierung.
- Staging, ACF schreiben, Updates per Manifest-Diff, Fortsetzen nach Abbruch.

**Fertig, wenn:** `reborn-cli install <appid>` ein Spiel installiert, das der Original-Client als vollständig installiert erkennt und dessen „Dateien überprüfen“ keine Fehler findet.

> Stand 2026-11-19: erfüllt für Spacewar (App 480). Updates per Manifest-Diff, Fortsetzen und Prüfen/Reparieren sind umgesetzt (`reborn-cli install`, `reborn-cli verify`); ein echtes Versions-Update wurde live noch nicht beobachtet. Offen: Install-Skripte (z.B. Redistributables).

> Ab hier ist das Projekt öffentlich vorzeigbar. Das ist der richtige Moment, um Mitwirkende zu gewinnen.

### M2 – Daemon und GUI → Release 0.1

- `rebornd` mit IPC; CLI läuft über den Daemon.
- Iced-GUI: Login, Bibliothek, Details, Downloads, Einstellungen.
- Spielstart: direkt für Spiele ohne Steamworks, sonst über den Original-Client (Stufe 1).
- Linux: Proton-Verwaltung und -Start.
- Kompatibilitätsdatenbank und Report-Vorlage.

**Fertig, wenn:** Ein Nutzer ohne Kommandozeile Spiele installieren, aktualisieren und starten kann, auf Windows und Linux. (Erreicht und live geprüft am 2026-11-20; Deinstallieren ergänzt (ADR 0018, live geprüft 2026-11-20); Release 0.1 vorbereitet: `CHANGELOG.md`, Release-Workflow `.github/workflows/release.yml`, der bei einem Tag `v*` einen Entwurf mit Programmen für Windows und Linux anlegt.)

Etappen (Stand 2026-11-19):

1. **`reborn-core`** (erledigt): Anmelden, Bibliothek, Installieren/Aktualisieren/Prüfen als gemeinsame Logik für CLI, Daemon und GUI; Fortschritt über Ereignisse statt Konsolenausgabe.
2. **`rebornd` + `reborn-ipc`** (erledigt, live geprüft 2026-11-19): lokale Schnittstelle (ADR 0008), Download-Warteschlange, Abbrechen, Fortschrittsereignisse; `reborn-cli daemon …` als erste Nutzerin. Die Named Pipe unter Windows ist seit ADR 0014 auf den eigenen Benutzer beschränkt.
3. **GUI `reborn`** mit Iced 0.14 (ADR 0009): erste Version gebaut und live geprüft (2026-11-19) mit Bibliothek (installiert/nicht installiert, Installieren/Aktualisieren/Prüfen, Suche) und Downloads (Fortschritt, Abbrechen); startet `rebornd` bei Bedarf selbst. Beim Test gefunden und behoben: Zwischendateien wurden in voller Größe vorbelegt (83 GB für einen abgebrochenen Download); sie wachsen jetzt nur mit den Daten (live geprüft 2026-11-20). Anmelden in der GUI über den Daemon mit Passwort und Steam Guard (Code oder Bestätigung in der App), Abmelden und „Beenden“ (ADR 0014, live geprüft 2026-11-20); dafür ist die Named Pipe unter Windows jetzt auf den eigenen Benutzer beschränkt. Offen: QR-Login (wartet auf steam-vent), Prüfung des Pipe-Servers durch das Frontend, Einstellungen, Spieldetails mit Artwork.
4. **Spiele starten**, Linux mit Proton; Kompatibilitätsdatenbank → Release 0.1. Stand 2026-11-20: Starten über den Daemon mit Play-Knopf in der GUI (ADR 0016), live geprüft mit Bongo Cat (Installieren, Starten, Beenden). Unter Linux (Debian-13-VM) live geprüft: Bauen, GUI, Anmelden mit Secret Service, Installieren und nativer Start von Bongo Cat; dabei gefunden und behoben: Reborn macht wie der Original-Client alle installierten Dateien ausführbar, auch wenn das Manifest keine so markiert. Kompatibilitätsdatenbank (ADR 0015) und Proton-Erkennung unter Linux sind gebaut; Windows-Spiele starten unter Linux über die neueste installierte Proton-Version (ADR 0017, noch ohne Steam-Linux-Runtime-Container); solche Spiele bekommen unter Linux ihre Windows-Depots. Live geprüft (2026-11-20, Debian-13-VM mit Software-Vulkan): Quatro! (nur Windows) installiert und startet über Proton Experimental, ohne Runtime-Container. Steam zeigt das Spiel als „spielt gerade“ an, und das Steam-Inventar funktioniert (eine Truhe ließ sich öffnen; ein erster Versuch war an einem vorübergehenden Fehler gescheitert).

### M3 – Social → Release 0.2

Freunde, Online-Status, Chat, Benachrichtigungen, Freundschaftsanfragen, Achievements anzeigen.

**Fertig, wenn:** Man im Alltag über Reborn chatten kann.

### M4 – Shim Stufe B1 → Release 0.3 „Unabhängigkeit für Singleplayer“

Shim für Windows 64-Bit und Linux, Testspiel, Cloud-Sync, Achievements aus dem Spiel heraus.

**Fertig, wenn:** Eine vorab festgelegte Liste von mindestens 20 verbreiteten Singleplayer-Spielen auf einem System **ohne installierten Original-Client** Stufe 3 erreicht.

### M5 – Shim B2 und B3: Multiplayer

Lobbys, Matchmaking, Rich Presence, Steam-Networking, 32-Bit-Windows.

**Fertig, wenn:** Eine festgelegte Liste von Koop- und Multiplayer-Spielen Stufe 4 erreicht.

### M6 – Workshop, Store, Feinschliff → Release 1.0

Workshop-Abos und -Downloads, Store zum Stöbern, Übersetzungen, Paketierung (MSI/winget, Flatpak, AUR), Sicherheitsprüfung, Rechtsprüfung.

**Fertig, wenn:** Alle Punkte aus 5.1 umgesetzt sind und keine bekannten Datenverlust- oder Sicherheitsfehler offen sind.

### Danach

Overlay, Steam Input, Controller-Oberfläche, SteamOS-Gaming-Modus, macOS.

### Zum Zeitplan

Es gibt bewusst keine Termine. Das Tempo hängt davon ab, wie viele Leute mitmachen. M0 und M1 sind mit Referenzimplementierungen und KI-Unterstützung eine Sache von Wochen, nicht von Monaten. M4 und M5 sind über Jahre angelegt.

---

## 12. KI-gestützte Entwicklung („Vibe-Coding“)

Das Projekt wird überwiegend mit KI-Agenten entwickelt. Das funktioniert gut, wenn man ehrlich damit umgeht, wo KI stark und wo sie gefährlich ist.

### 12.1 Stärken und Schwächen

| KI ist stark bei | KI ist gefährlich bei |
| --- | --- |
| GUI, Boilerplate, CLI, Einstellungen | undokumentierten Protokollen (erfindet Felder, EMsg-Nummern, Abläufe) |
| Parsern, wenn Testdaten vorliegen | Krypto-Details (Modi, Padding, Schlüsselableitung) |
| Tests, Dokumentation, Refactoring | ABI und vtable-Layouts (Teil B) |
| Plattform-Glue mit gut dokumentierten APIs | Sicherheitskritischem Code (Tokens, Dateipfade) |

### 12.2 Regeln für Protokollcode

1. **Protobufs werden generiert, nie von Hand geschrieben.**
2. **Kein Protokollverhalten ohne Quelle.** Jede Implementierung verweist im Code-Kommentar oder in `docs/protocol/` auf eine Referenz (Projekt, Datei, Zeile) oder auf ein aufgezeichnetes Fixture.
3. **Kein Fixture, kein Merge.** Protokollcode braucht einen Replay-Test gegen echten, aufgezeichneten Verkehr.
4. **Fixtures werden automatisch geschwärzt.** Tokens, Steam-IDs, E-Mail-Adressen und IP-Adressen werden vor dem Commit ersetzt. Ein CI-Check sucht nach Mustern, die nach Geheimnissen aussehen.
5. **Keine Spieldateien im Repository.** Manifeste (Metadaten) sind in Ordnung, Chunk-Inhalte nicht.

### 12.3 Ablauf

- Jedes Issue beschreibt eine kleine, abgeschlossene Aufgabe mit Akzeptanzkriterien und erwartetem Test. Agenten arbeiten Issue für Issue.
- `AGENTS.md` im Repository enthält die verbindlichen Regeln: Architektur, rote Linien, Teststandards, verbotene Muster.
- Agenten ändern keine öffentliche API von `proto`, `cm`, `content` oder `ipc` ohne ADR.
- Mehrere Agenten arbeiten nie gleichzeitig an denselben Dateien.
- Live-Tests gegen Valve laufen nur manuell, mit Zweitaccount, nie in Schleifen und nie in CI.

### 12.4 Menschliches Review ist Pflicht für

- Login, Tokens, Schlüsselbund,
- alles, was Dateien schreibt oder löscht,
- die Shim (ABI, `unsafe`),
- alles, was im Namen des Accounts handelt (Nachrichten, Freundschaftsanfragen, Cloud-Uploads).

`CODEOWNERS` setzt das technisch durch.

### 12.5 Qualitäts-Gates in CI

`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `cargo deny check`, Secret-Scan auf Fixtures und Builds für Windows und Linux.

### 12.6 Haltung gegenüber Beiträgen

Viele Open-Source-Communities reagieren allergisch auf unkontrollierte KI-PRs. Deshalb gilt: KI-Unterstützung ist ausdrücklich willkommen, aber **wer einen PR einreicht, ist für jede Zeile verantwortlich** und hat den Code selbst ausgeführt. Ungeprüfte Massen-PRs werden geschlossen.

---

## 13. Community und Organisation

### 13.1 Was Leute anzieht

Communities entstehen um funktionierende Software, nicht um Pläne. Daher:

1. M0 und M1 schnell öffentlich machen. Ein Spiel, das komplett ohne Steam-Client in Rust heruntergeladen wird, ist eine starke Demo.
2. Eine klare, sichtbare Roadmap (GitHub Projects).
3. Viele gut beschriebene Einsteiger-Issues.
4. Schnelle, freundliche Reviews.
5. Ehrliche Kommunikation über Grenzen und Risiken.

### 13.2 Einstiegsmöglichkeiten

| Bereich | Vorkenntnisse |
| --- | --- |
| Kompatibilitätsberichte | keine |
| Übersetzungen (Fluent) | keine |
| Dokumentation, Guides | wenig |
| GUI-Komponenten | Rust-Grundlagen |
| Parser-Randfälle (VDF, ACF) | Rust-Grundlagen |
| Plattform-Code (Pfade, Prozesse, Proton) | mittel |
| Protokoll, Content-Engine | fortgeschritten |
| Shim | Experten |

### 13.3 Organisation

- **Anfang:** ein Maintainer (Projektgründer) mit dem letzten Wort.
- **Mit Wachstum:** Maintainer pro Bereich (Protokoll, Content, GUI, Linux, Shim), festgehalten in `GOVERNANCE.md`.
- **Größere Vorhaben:** ein leichtgewichtiger RFC (Issue-Vorlage), der in ein ADR mündet.
- **Kommunikation:** GitHub Discussions, dazu Matrix oder Discord.
- **Verhaltenskodex:** Contributor Covenant.

### 13.4 Repository-Ablauf

- Trunk-basiert: `main` plus kurzlebige Feature-Branches, Squash-Merge. Kein `develop`-Branch.
- Conventional Commits; automatisierte Changelogs und Releases (z.B. release-plz).
- Issue-Vorlagen: Bug, Kompatibilitätsbericht, Feature-Wunsch, RFC.
- Labels: `good first issue`, `help wanted`, `area/*`, `compat/*`, `needs-fixture`.
- Sicherheitslücken nur privat melden (GitHub Security Advisories), beschrieben in `SECURITY.md`.

---

## 14. Risiken

| Risiko | Wahrscheinlichkeit | Auswirkung | Gegenmaßnahme |
| --- | --- | --- | --- |
| Valve ändert das Protokoll | hoch | mittel | Generierte Protobufs, wöchentlicher Update-Check, Fixtures zeigen Brüche sofort |
| Valve sperrt Drittanbieter-Clients | niedrig | sehr hoch | Rote Linien einhalten, höflicher Client, keine Missbrauchsfunktionen |
| Account-Sperren einzelner Nutzer | niedrig | hoch | Warnhinweis, Zweitaccount für Entwicklung, keine aggressiven Muster |
| Takedown wegen Marke oder Assets | mittel | hoch | Kein „Steam“ im Namen, keine Valve-Grafiken, Disclaimer |
| Schlechte Qualität durch KI-Code | hoch | mittel | Fixtures, Gates, Pflicht-Reviews, kleine Issues |
| Shim wird zu komplex | hoch | hoch | Stufenweise, Testspiel, Kompatibilitätsdatenbank steuert Prioritäten |
| Kernel-Anti-Cheat-Spiele laufen nicht | hoch | mittel | Ehrlich als „nicht unterstützt“ kennzeichnen, Fallback auf Original-Client |
| Maintainer-Burnout, Bus-Faktor 1 | mittel | sehr hoch | Früh Bereichs-Maintainer gewinnen, gute Doku, keine Termine versprechen |
| Scope Creep | hoch | mittel | Streichliste (5.3), Prinzip „minimal“ |

---

## 15. Architekturentscheidungen (ADRs)

Ablage in `docs/adr/`. Jedes ADR enthält Kontext, Problem, Alternativen, Entscheidung und Konsequenzen.

| Nr. | Thema |
| --- | --- |
| 001 | Interoperabilität durch Reverse Engineering und rote Linien |
| 002 | Daemon/Frontend-Trennung |
| 003 | Lizenzaufteilung GPL / MIT-Apache |
| 004 | Projektname: Reborn, GitHub `playreborn`, crates.io `reborn` |
| 005 | Steam-Dateiformate als Wahrheit für Installationen |
| 006 | CM-Transport: WebSocket zuerst |
| 007 | Eigenes `cm`-Crate vs. `steam-vent` |
| 008 | IPC-Format Frontend ↔ Daemon |
| 009 | Iced als GUI-Framework |
| 010 | Speicherung von Zugangsdaten |
| 011 | Herkunft der Protobuf-Definitionen |
| 012 | Shim-Ansatz (vtables, Versionen, IPC) |

---

## 16. Nächste Schritte

- [x] Name festgelegt: Reborn. GitHub-Organisation `playreborn` und crates.io-Name `reborn` gesichert.
- [ ] Kurze Markenrecherche (DPMA/EUIPO, Klasse Software) und Domain prüfen.
- [ ] Zweitaccount für Entwicklung und Tests anlegen.
- [ ] Repository `playreborn/reborn` anlegen, Lizenzdateien, README mit Unterzeile und Disclaimer.
- [x] Workspace-Gerüst nach 6.3, `rust-toolchain.toml`, CI mit fmt, Clippy, Tests und cargo-deny. (Secret-Scan: GitHub Secret Scanning und Push Protection in den Repo-Einstellungen aktivieren.)
- [x] `AGENTS.md`, `CONTRIBUTING.md` (mit DCO), `SECURITY.md`, `CODE_OF_CONDUCT.md`, ADRs 0001–0004.
- [x] Lizenzdateien `LICENSE-GPL`, `LICENSE-APACHE`, `LICENSE-MIT`.
- [x] Protobufs kommen generiert über `steam-vent-proto-steam` (Quelle: SteamDatabase/Protobufs, ADR 0007).
- [x] `steam-vent` evaluiert und als Basis gewählt (ADR 0007). `steamlocate` ist noch offen.
- [x] Spike: Login mit Passwort + Steam Guard über WebSocket-CM, Refresh-Token im Schlüsselbund (ADR 0007, 0010). QR-Login braucht einen Beitrag zu steam-vent.
- [ ] Fixture-Recorder mit automatischer Schwärzung.
- [x] `reborn-formats` 0.1.0 als erstes echtes Crate auf crates.io veröffentlicht.
- [x] `reborn-cli login` / `library` live mit dem Zweitaccount getestet: Login per App-Bestätigung, Token-Login nach Neustart (2026-11-19).
- [ ] Repository öffentlich machen, Roadmap und erste Einsteiger-Issues anlegen.

---

## Anhang: Glossar

| Begriff | Bedeutung |
| --- | --- |
| CM | Connection Manager, Valves Server, mit denen der Client dauerhaft verbunden ist |
| PICS | Product Info and Change System, liefert App- und Paket-Metadaten |
| Depot | Inhaltspaket einer App (z.B. pro Plattform oder Sprache) |
| Manifest | Liste der Dateien und Chunks eines Depots in einer bestimmten Version |
| Chunk | Verschlüsseltes, komprimiertes Datenstück (bis ca. 1 MB) auf dem CDN |
| ACF / VDF | Valves KeyValues-Textformat (z.B. `appmanifest_*.acf`) bzw. dessen Binärvariante |
| Steamworks | Valves SDK, das Spiele für Achievements, Multiplayer usw. nutzen |
| steamclient | Die Bibliothek des Clients, mit der `steam_api` im Spiel spricht |
| Shim | Unser Ersatz für `steamclient` |
| SDR | Steam Datagram Relay, Valves Relay-Netz für Multiplayer |
| Proton | Valves Wine-basierte Kompatibilitätsschicht für Windows-Spiele auf Linux |

---

*Ende des Projektplans – v2.0*
