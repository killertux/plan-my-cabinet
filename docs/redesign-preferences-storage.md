# App-local preferences storage

`src/local_preferences.rs` stores the **General** display preferences in a
versioned `preferences.json` under an absolute configuration directory supplied
by the desktop host. It never derives a path from the current directory or a
project filename. The caller must supply the platform app configuration
directory (and an isolated directory for native captures/tests). A missing file
loads English, hints on, normal zoom, material tint on, and 100% scale.

`PreferencesStore::load` returns a `PreferencesLoad` with a diagnostic when the
file cannot be read or is malformed, oversized, contains duplicate or unknown
fields, or has an unsupported version/scale. In that case it uses defaults
without overwriting the suspect file. The file is limited to 16 KiB; supported
scale values are numeric 90, 100, 115 and 130. A failed save is returned to the
caller for visible reporting. Writes use the existing private, same-directory
temporary-file + flush/fsync + atomic rename + directory-fsync sequence; a
post-rename sync failure is reported as durability uncertainty because the new
bytes may already be present.

The native desktop now loads from the platform configuration directory before
configuring locale and egui zoom, reports load/save warnings, and persists
language changes immediately without touching the project editor. Its saved
inverse-scroll choice drives viewport zoom; isolated capture runs do not read
or write the user's preferences. Export language, project grid/kerf/costs and
portable material sRGB colors remain independent. General Settings controls,
and live navigation-hint and material-tint behavior, remain task 14.3; storage
and round-trip tests do not certify those unfinished controls.
