# Quatsch mit Soße – Masterplan mit Copy-Paste-Prompts

Stand 17.11.2026 · Version 3 · ersetzt Version 2 (Projektname, Pfad und VS Code ergänzt) · Die Informationssammlung (PDF mit Screenshots) bleibt als Hintergrund gültig, die Korrekturen stehen in Teil A.

Ziel: eine deutsche, offline-first Diktier-App für Windows 11 im Stil von Wispr Flow. Spracherkennung und Bereinigung laufen lokal, KI über dein ChatGPT-Konto ist optional.

## So benutzt du dieses Dokument

Jeder Bauschritt hat einen fertigen Prompt im grauen Kasten. Du kopierst ihn, fügst ihn in Codex (oder Claude Code) ein und wartest. Alles, was du selbst tun musst, ist mit **Du:** markiert.

Ablauf für jeden Schritt:

1. VS Code starten, den Ordner `H:\Quatsch mit Soße` öffnen (Datei → Ordner öffnen), das Terminal in VS Code öffnen (Strg+Ö auf deutscher Tastatur) und `codex` eingeben. Für Claude Code stattdessen `claude`. Bevorzugst du die Codex-Erweiterung von OpenAI für VS Code, funktionieren dieselben Prompts auch dort.
2. Für jeden Schritt eine neue Sitzung beginnen (in Codex `/new`, in Claude Code `/clear`). Die Prompts sind so geschrieben, dass sie ohne Vorwissen funktionieren.
3. Prompt aus dem Kasten einfügen und Enter drücken. Wenn Codex um Erlaubnis fragt (Befehl ausführen, Datei schreiben, Download), mit Ja bestätigen.
4. Danach die Zeile **Du prüfst** abarbeiten. Das dauert meist eine Minute.
5. Ist alles in Ordnung: Standard-Prompt **C (Commit)** einfügen. Gibt es Fehler: Standard-Prompt **F (Fehler beheben)**. Bricht Codex mittendrin ab: Standard-Prompt **W (Weiter)**. Die Standard-Prompts stehen am Ende.
6. Nach jeder Phase einmal Standard-Prompt **R (Review)**.

Regeln, die dir Ärger ersparen:

- Nie zwei Schritte in einem Prompt kombinieren. Kleine Schritte sind schneller und einfacher zu prüfen.
- Reihenfolge einhalten. Spätere Schritte bauen auf früheren auf.
- Keine Aufnahmen deiner Stimme ins öffentliche Repository. Der Ordner `testdata/private/` ist dafür reserviert und wird ignoriert.
- Wenn ein Prompt nach einer Entscheidung fragt, steht die Empfehlung im Prompt. Du kannst einfach „Nimm die Empfehlung“ antworten.

## Teil A – Informationssammlung kompakt

### Was Wispr Flow ist

- Cloud-App: Audio geht an Wispr-Server, wird dort transkribiert und von Cloud-KI nachbearbeitet. Einen Offline-Modus gibt es nicht.
- Stil-Formatierung (Formal, Casual, very casual) funktioniert laut eigener Oberfläche nur auf Englisch.
- Preise: Free mit Wortlimit, Pro 15 € pro Monat, Enterprise ab 23 €. Für Privatpersonen relevant sind Free und Pro.

### Bekannte Wispr-Probleme und die Lösung in Quatsch mit Soße

| Problem bei Wispr | Lösung in Quatsch mit Soße |
| --- | --- |
| Electron-App, laut Nutzerberichten ca. 800 MB RAM und mehrere Prozent CPU im Leerlauf | Tauri + Rust, Modell wird bei Leerlauf entladen, Ziel unter 100 MB |
| Nur Cloud, Audio verlässt den Rechner | Spracherkennung lokal, Netzwerk nur nach Freigabe |
| Kontext-Funktion stand in der Kritik (laut Berichten Screenshots an Server) | Keine Screenshots, Kontext nur lokal über Windows UI Automation |
| Standard-Hotkey Ctrl+Win kollidiert, in Administrator-Fenstern keine Reaktion | Hotkey mit normaler Taste, Konfliktwarnung, Hinweis bei Admin-Fenstern |
| Einfügen scheitert in manchen Apps, Zwischenablage wird überschrieben | Drei Einfügewege nacheinander, Zwischenablage wird vollständig wiederhergestellt |
| Probleme mit Nicht-QWERTY-Layouts | Layoutunabhängige Unicode-Eingabe, Tests mit QWERTZ |
| Erste Wörter fehlen | Mikrofon vorgewärmt, 500 ms Vorlaufpuffer |
| Auto-Cleanup ändert ungewollt Wörter | Konservative Regeln, jede einzeln abschaltbar, Original per Hotkey zurück |
| Stil nur Englisch | Deutsches Regelwerk mit Du/Sie |
| Notetaker: Einwilligungshinweise aus, Links für jeden mit Link | Hinweise an, kein Link-Teilen, nur Export |
| Bei Stille erfundene Sätze (typisch für Whisper-Modelle) | Sprachaktivitätserkennung und Sperrliste |

### Dein Nutzungsprofil (aus den Screenshots)

14.000 Wörter, 126 Wörter pro Minute, 56 % KI-Prompts, 36 % sonstige Aufgaben, 5 % E-Mails. Wichtig für dich sind deshalb: zuverlässiges deutsches Langdiktat, „Gedanken ordnen“ und „Prompt bauen“ auf Tastendruck, Fachbegriffe wie Codex im Wörterbuch, sauberes Einfügen in Browser, Editoren und KI-Chats.

### Was sich seit Version 1 geändert hat

Ich habe die wichtigsten Punkte am 17.11.2026 neu geprüft. Diese Korrekturen sind bereits in alle Prompts eingearbeitet.

1. **Keine Client-ID anfragen.** Für Open-Source- und lokal laufende Apps registriert sich die App beim ersten Login selbst: Sie startet mit `client_id=dynamic_agent_client` und bekommt eine eigene Client-ID zurück. Laut OpenAI braucht dieser Weg weder ein Client-Secret noch einen API-Schlüssel. Nur für bezahlte oder gehostete Apps gibt es ein Interessenformular.
2. **Einschränkungen der ChatGPT-Plan-Nutzung (Preview):** Anfragen müssen `store: false` und `stream: true` setzen; Felder wie `temperature` oder `max_output_tokens` sind nicht erlaubt; Audio-Eingabe und die Transkriptions-API werden über diesen Weg nicht unterstützt. Die Spracherkennung muss also ohnehin lokal laufen, was zum Plan passt.
3. **Deutsches Parakeet ist fertig verfügbar.** Es gibt einen sherpa-onnx-kompatiblen int8-Export des deutschen Fine-Tunes primeline/parakeet-primeline, erstellt für eine Windows-Diktier-App ohne GPU, Lizenz CC-BY-4.0. Die Basisversion Parakeet TDT 0.6B v3 (int8) bietet sherpa-onnx offiziell zum Download an, ebenfalls CC-BY-4.0. CC-BY bedeutet: Namensnennung in der App ist Pflicht.
4. **Eine Laufzeit für alles:** sherpa-onnx kann Parakeet, Whisper und die Sprachaktivitätserkennung (Silero) ausführen. Für Rust gibt es die Bindung `sherpa-rs`. Das spart viel Arbeit gegenüber drei getrennten Laufzeiten. Vergleichbare Open-Source-Projekte für Windows nutzen genau diese Kombination.
5. **Codex unter Windows:** läuft nativ in PowerShell mit eigener Windows-Sandbox und liest `AGENTS.md` automatisch. Claude Code liest `CLAUDE.md`; diese Datei importiert einfach `@AGENTS.md`, damit beide Werkzeuge dieselben Regeln nutzen.

Alle Zahlen zur Genauigkeit sind weiterhin Herstellerangaben. Welches Modell Standard wird, entscheidet der Benchmark in Phase 1 mit deinen eigenen Aufnahmen.

## Teil B – Festlegungen und Architektur

Diese Entscheidungen sind fest, damit Codex nicht bei jedem Schritt neu wählt.

| Bereich | Festlegung |
| --- | --- |
| Plattform | Windows 11 x64, CPU-Betrieb als Standard |
| App-Rahmen | Tauri 2, Rust-Workspace, Oberfläche mit Svelte 5 + TypeScript + Vite |
| Audio | WASAPI über die Rust-Crate `cpal`, 16 kHz mono, 500 ms Vorlaufpuffer |
| Spracherkennung | sherpa-onnx über `sherpa-rs`. Kandidaten: Parakeet TDT 0.6B v3 int8 (Basis), parakeet-primeline-onnx (Deutsch), Whisper large-v3-turbo (Rückfall). Standard nach Benchmark |
| Sprachaktivität | Silero-VAD über sherpa-onnx |
| Bereinigung | Eigenes Regelwerk in Rust, KI nur optional |
| KI | Anbieter Aus (Standard), Lokal (llama.cpp-Server), ChatGPT (Sign in with ChatGPT) |
| Daten | SQLite über `rusqlite` mit Volltextsuche (FTS5), Geheimnisse im Windows-Anmeldeinformationsspeicher |
| Einfügen | UI Automation → Unicode-Tastatureingabe → Zwischenablage mit Wiederherstellung |
| Lizenz | Apache-2.0, öffentliches GitHub-Repository, Datei NOTICE mit allen Modell-Lizenzen |

### Name, Ordner und technische Namen

Der Anzeigename darf Leerzeichen und ß enthalten. Alles, was in Dateinamen, Crates, Adressen und Bezeichnern landet, bleibt ASCII, weil Leerzeichen und ß dort in Werkzeugen, Installern und GitHub-Adressen Probleme machen.

| Was | Wert |
| --- | --- |
| Arbeitstitel und Anzeigename | Quatsch mit Soße |
| Projektordner | `H:\Quatsch mit Soße` |
| Technischer Name | quatsch-mit-sosse |
| GitHub-Repository | quatsch-mit-sosse |
| Tauri-Bezeichner | de.quatschmitsosse.app |
| Datenordner | `%LOCALAPPDATA%\QuatschMitSosse` |
| Datenbank | `%LOCALAPPDATA%\QuatschMitSosse\data\quatsch-mit-sosse.db` |
| Log-Marker für das Messskript | QMS\_READY |
| Pipe zur IDE-Erweiterung | `\\.\pipe\quatsch-mit-sosse-ide` |
| MCP-Programm | quatsch-mit-sosse-mcp.exe |
| Build-Ausgabe von Rust | `C:\qms-target` (siehe Schritt V5) |
| Arbeitsumgebung | VS Code mit Codex im Terminal oder in der Codex-Erweiterung |

### Ordnerstruktur

```
Quatsch mit Soße/
  AGENTS.md          Regeln für Codex
  CLAUDE.md          verweist auf AGENTS.md (für Claude Code)
  NOTICE             Lizenzen und Namensnennungen der Modelle
  docs/              masterplan.md, decisions/, benchmarks/
  crates/            audio, vad, stt, models, textfx, hotkey, inject,
                     context, store, llm, notes, mcp
  app/               Tauri-App (src-tauri/ und ui/)
  tools/             bench, corpus-recorder
  testdata/public/   synthetische Testdateien
  testdata/private/  deine Aufnahmen (nie im Repository)
  scripts/           Messskripte
```

### Datenfluss eines Diktats

```
Hotkey → audio (Vorlauf + Stream) → vad → stt → textfx → [optional llm] → inject → store
```

Zustände: Bereit → Hört zu → Verarbeitet → Fügt ein → Bereit. Bei jedem Fehler landet der Text im Verlauf und ist per „Letzten Text einfügen“ abrufbar.

### Release-Fahrplan

- **v0.1 (Phase 0–2):** Offline-Diktat mit Hotkey, Overlay, sicherem Einfügen. Ab hier ersetzt die App Wispr für dich im Alltag.
- **v0.5 (Phase 3–4):** Bereinigung, Wörterbuch, Snippets, Verlauf, Einstellungen, Scratchpad.
- **v0.8 (Phase 5–7):** Stil, Kontext, KI-Anbieter, Transforms, Vibe-Coding.
- **v1.0 (Phase 8):** Insights, Härtung, Installer.
- **v1.5 (Phase 9):** Notetaker und lokaler MCP-Server.

## Teil C – Einmalige Vorbereitung

### V1 · Codex installieren

**Du:** Im Explorer den Ordner `H:\Quatsch mit Soße` anlegen. In VS Code über Datei → Ordner öffnen diesen Ordner öffnen, dann das Terminal öffnen (Strg+Ö) und die folgenden Zeilen einfügen. Der Installer und die Anmeldung kommen von OpenAI; bei der Frage nach der Sandbox die Standard-Option wählen. Findet das Terminal `codex` danach nicht, Terminal schließen und neu öffnen.

```
powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 | iex"
codex
```

Beim ersten Start „Sign in with ChatGPT“ wählen und mit deinem Pro-Konto anmelden. Falls du Claude Code nutzt: nach der offiziellen Anleitung installieren und im selben Ordner `claude` starten.

### V2 · Werkzeuge einrichten

**Du:** In Codex einfügen. Bei Windows-Abfragen („Möchten Sie zulassen …“) mit Ja bestätigen.

```
Richte auf diesem Windows-11-Rechner die Entwicklungsumgebung für ein Tauri-2-Projekt mit Rust und Svelte ein. Prüfe zuerst mit Befehlen, was schon installiert ist, und installiere nur Fehlendes über winget:
- Git (Git.Git)
- Rust über rustup (Rustlang.Rustup), danach: rustup default stable-msvc
- Visual Studio 2022 Build Tools mit dem Workload "Desktopentwicklung mit C++" (Microsoft.VisualStudio.2022.BuildTools, Workload Microsoft.VisualStudio.Workload.VCTools inklusive Windows SDK)
- Node.js LTS (OpenJS.NodeJS.LTS)
- GitHub CLI (GitHub.cli)
- LLVM/Clang (LLVM.LLVM), wird von manchen Rust-Bindungen zum Bauen gebraucht
WebView2 ist unter Windows 11 vorinstalliert; prüfe es nur.
Installiere außerdem über "code --install-extension" diese VS-Code-Erweiterungen: rust-lang.rust-analyzer, tamasfe.even-better-toml, svelte.svelte-vscode, tauri-apps.tauri-vscode, vadimcn.vscode-lldb. Wird eine Kennung nicht gefunden, überspringe sie und sag es mir.
Wenn eine Installation ein neues Terminal braucht, sag mir das deutlich. Zum Schluss eine Tabelle: Werkzeug, Version, Status. Lege keine Projektdateien an.
```

**Du prüfst:** Alle Zeilen der Tabelle stehen auf OK. Danach VS Code komplett schließen, neu öffnen (Ordner `H:\Quatsch mit Soße`), das Terminal öffnen (Strg+Ö) und `codex` starten.

### V3 · Masterplan ins Projekt legen

**Du:** Dieses Dokument als Markdown herunterladen und speichern als `H:\Quatsch mit Soße\docs\masterplan.md` (Ordner `docs` vorher anlegen, in VS Code per Rechtsklick im Explorer → Neuer Ordner). Optional auch `Flow-DE_Informationssammlung.pdf` (so heißt die Datei noch) nach `docs\` legen.

### V4 · Projekt anlegen

**Du:** `gh auth login` im VS-Code-Terminal ausführen und mit GitHub anmelden. Dann in Codex einfügen:

```
Lege das Projekt "Quatsch mit Soße" an (technischer Name: quatsch-mit-sosse). Der Projektordner ist dein aktuelles Arbeitsverzeichnis. docs/masterplan.md existiert bereits, lies es (Teil B) vorher.

1. git init, Branch main.
2. Lege AGENTS.md mit exakt diesem Inhalt an:

# Quatsch mit Soße – Regeln für Coding-Agenten
## Projekt
Deutsche, offline-first Diktier-App für Windows 11 (Tauri 2, Rust-Workspace, Svelte 5 + TypeScript). Der Plan steht in docs/masterplan.md. Bei Widersprüchen gilt der Masterplan; begründete Abweichungen kommen nach docs/decisions/.
## Sprache
UI-Texte, Doku und Commit-Nachrichten auf Deutsch. Code, Bezeichner und Kommentare auf Englisch.
## Datenschutz (nicht verhandelbar)
- Keine Telemetrie, keine Analytics, keine Absturzberichte nach außen.
- Keine Screenshots und keine Aufrufe von Bildschirmaufnahme-APIs.
- Netzwerk nur für Modell-Downloads mit SHA256-Prüfung und für ausdrücklich aktivierte KI-Anbieter.
- Diktierten Text, Audio und Tokens nie loggen.
- Nie committen: testdata/private/, Modelldateien, Zugangsdaten.
## Arbeitsweise
- Genau eine Aufgabe pro Lauf, nichts darüber hinaus.
- Erst Tests schreiben, dann implementieren.
- Vor Abschluss müssen grün sein: cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test --workspace, und in app/ui: npm run check und npm test.
- Bei Unklarheiten die Empfehlung aus dem Masterplan nehmen und in docs/decisions/ notieren.
- Zum Schluss auf Deutsch zusammenfassen: was geändert, wie getestet, was offen ist, was der Mensch prüfen soll.
## Technik
- Spracherkennung über sherpa-onnx (Crate sherpa-rs) hinter dem Trait in crates/stt.
- Windows-APIs über die Crate windows.
- Einfügen in dieser Reihenfolge: UI Automation, SendInput mit KEYEVENTF_UNICODE, Zwischenablage mit vollständiger Wiederherstellung.
- Anzeigename "Quatsch mit Soße", technischer Name quatsch-mit-sosse (in Dateinamen, Crates, Adressen und Bezeichnern nur ASCII). Tauri-Bezeichner de.quatschmitsosse.app.
- Der Projektordner H:\Quatsch mit Soße enthält Leerzeichen und ß: Pfade in Skripten und Befehlen immer in Anführungszeichen setzen. Rust-Build-Ausgabe über CARGO_TARGET_DIR=C:\qms-target.
- Modelle unter %LOCALAPPDATA%\QuatschMitSosse\models, Daten unter %LOCALAPPDATA%\QuatschMitSosse\data.
- Leistungsziele: Leerlauf unter 100 MB RAM ohne geladenes Modell, unter 1 % CPU, Start unter 2 Sekunden.

3. CLAUDE.md mit dem einzigen Inhalt: @AGENTS.md
4. LICENSE (Apache-2.0, Inhaber: Denis R.), NOTICE (vorerst nur Überschrift "Drittanbieter und Modelle"), README.md auf Deutsch (Kurzbeschreibung, Status "in Entwicklung").
5. .gitignore für Rust, Node, Tauri sowie testdata/private/, *.onnx, *.gguf, *.bin, models/.
6. Leere Ordner mit .gitkeep: docs/decisions, docs/benchmarks, testdata/public, testdata/private, scripts, tools.
7. Erster Commit "Projekt angelegt". Dann mit gh ein öffentliches GitHub-Repository quatsch-mit-sosse anlegen und pushen.
Noch keinen Code, kein Cargo-Workspace.
```

**Du prüfst:** Das Repository ist auf GitHub sichtbar und enthält AGENTS.md, LICENSE und docs/masterplan.md, aber keinen Ordner testdata/private mit Inhalt.

#### V5 · Pfad mit Leerzeichen und ß absichern

Der Ordner `H:\Quatsch mit Soße` enthält Leerzeichen und ein ß. Windows kommt damit zurecht, manche Build-Werkzeuge (Rust-Bindungen mit C++-Bauskripten, Installer-Bauer) nicht immer. Dieser Schritt prüft das einmal und legt die Build-Ausgabe sicherheitshalber auf einen reinen ASCII-Pfad.

```
Lies AGENTS.md. Der Projektordner H:\Quatsch mit Soße enthält Leerzeichen und ein ß. Sorge dafür, dass Rust, Node und Tauri damit sicher arbeiten:
1. Setze für meinen Windows-Benutzer dauerhaft die Umgebungsvariable CARGO_TARGET_DIR auf C:\qms-target (setx). Alle Rust-Build-Ausgaben liegen dann in einem ASCII-Pfad. Sag mir, dass ich danach das Terminal neu öffnen muss.
2. Prüfe und melde, ohne etwas zu ändern: Ist H: ein lokales NTFS-Laufwerk, kein Netzlaufwerk und keine Wechselplatte? Sind 8.3-Kurznamen auf H: aktiv (fsutil 8dot3name query H:; braucht das Administratorrechte, dann nur melden)?
3. Lege scripts/path-check.ps1 an. Es erzeugt unterhalb des Projekts einen temporären Ordner "Test Ordner ß" mit einem Hello-World-Rust-Projekt und einem leeren npm-Paket, führt cargo build und npm install aus, meldet das Ergebnis und löscht den Ordner wieder. Führe es aus.
4. Alle Skripte und Befehle in diesem Projekt setzen Pfade in Anführungszeichen. Ergänze das in AGENTS.md unter Arbeitsweise, falls es dort noch fehlt.
Ändere sonst nichts. Bei Fehlern im Test: Ursache nennen und einen Vorschlag machen, aber nichts umbauen.
```

**Du prüfst:** Das Skript meldet erfolgreiche Builds. Schlägt es fehl, den Fehlertext mit Standard-Prompt **F** weitergeben. Danach den Stand mit Standard-Prompt **C** committen.

## Teil D – Bauschritte

### Phase 0 · Fundament

#### Schritt 0.1 · Workspace und App-Gerüst

```
Lies AGENTS.md und docs/masterplan.md (Teil B).
Aufgabe: Lege das technische Gerüst an, ohne Funktionen.
- Cargo-Workspace im Wurzelordner mit den Library-Crates aus der Ordnerstruktur (audio, vad, stt, models, textfx, hotkey, inject, context, store, llm, notes, mcp). Jede Crate hat eine lib.rs mit einem Platzhalter-Test.
- Tauri-2-App in app/ (app/src-tauri ist Workspace-Mitglied), Oberfläche in app/ui mit Svelte 5, TypeScript und Vite, Paketmanager npm. Produktname "Quatsch mit Soße" (Fenstertitel, Tray-Tooltip, Installer), Paketname quatsch-mit-sosse, Bezeichner de.quatschmitsosse.app. Der Projektordner enthält Leerzeichen und ß: Pfade in Skripten immer in Anführungszeichen.
- Tray-Symbol mit Menü "Einstellungen" und "Beenden". Einstellungsfenster startet versteckt und öffnet sich über das Tray.
- Overlay-Fenster als Platzhalter: rahmenlos, transparent, immer oben, nicht in der Taskleiste, standardmäßig versteckt.
- Nur eine Instanz der App darf laufen (Tauri-Plugin single-instance).
- Übersetzungen mit einer einfachen JSON-Datei pro Sprache (de als Standard, en), alle sichtbaren Texte aus diesen Dateien.
- Schließt man das Einstellungsfenster, wird es zerstört statt versteckt, damit der Speicher frei wird.
- In app/ui: npm-Skripte check (svelte-check) und test (vitest) mit je einem Beispieltest.
Keine weiteren Funktionen. Am Ende: Befehl nennen, mit dem ich die App im Entwicklungsmodus starte.
```

**Du prüfst:** Den genannten Befehl ausführen. Ein Tray-Symbol erscheint, „Einstellungen“ öffnet ein Fenster mit deutschem Text, „Beenden“ beendet die App.

#### Schritt 0.2 · Automatische Prüfungen (CI) und Datenschutz-Wächter

```
Lies AGENTS.md.
Aufgabe: GitHub-Actions-Workflow .github/workflows/ci.yml auf windows-latest mit: cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test --workspace, sowie in app/ui npm ci, npm run check, npm test. Cache für Cargo und npm.
Zusätzlich ein Skript scripts/privacy-guard.ps1, das lokal und in der CI läuft und fehlschlägt, wenn:
- eine Datei unter testdata/private/ oder eine Datei mit Endung .onnx, .gguf, .bin, .wav im Git-Index liegt (Ausnahme: testdata/public/*.wav),
- im Code Telemetrie-Crates oder -Pakete vorkommen (z. B. sentry, posthog, segment, mixpanel),
- im Code Bildschirmaufnahme-APIs vorkommen (z. B. BitBlt, PrintWindow, Windows.Graphics.Capture, CopyFromScreen),
- HTTP-Clients (reqwest, ureq, hyper) außerhalb des Moduls crates/models/src/net.rs bzw. crates/llm/src/net.rs verwendet werden.
Teste das Skript mit einem absichtlich verbotenen Beispiel in einem temporären Ordner und zeige, dass es anschlägt.
```

**Du prüfst:** Nach dem Push zeigt GitHub unter „Actions“ einen grünen Haken.

#### Schritt 0.3 · Messskript für Speicher, CPU und Startzeit

```
Lies AGENTS.md (Leistungsziele).
Aufgabe: scripts/measure-idle.ps1.
- Baut die App im Release-Modus (oder nimmt einen vorhandenen Build), startet sie und misst die Zeit bis zur Zeile "QMS_READY" im Log. Ergänze dafür in der App genau diese eine Logzeile, sobald Tray und Hotkey-Bereitschaft stehen.
- Wartet 60 Sekunden, misst dann 30 Sekunden lang Arbeitsspeicher (Private Bytes und Working Set) und CPU aller Prozesse der App, einschließlich der WebView2-Kindprozesse, und summiert sie.
- Gibt eine Tabelle aus und speichert JSON unter docs/benchmarks/idle-JJJJ-MM-TT.json.
- Beendet die App am Ende sauber.
Führe das Skript einmal aus und nenne die Werte im Vergleich zu den Zielen.
```

**Du prüfst:** Das Skript läuft durch und zeigt Zahlen. Werte über dem Ziel sind in Phase 0 noch normal; Codex soll sie nur nennen.

### Phase 1 · Audio und Spracherkennung

#### Schritt 1.1 · Mikrofon-Aufnahme

```
Lies AGENTS.md und docs/masterplan.md (Teil B).
Aufgabe: crates/audio.
- Aufnahme über cpal (WASAPI), Ausgabe immer 16 kHz mono f32 (Resampling, falls das Gerät anders liefert).
- Ständig laufender Ringpuffer von 500 ms, solange die Aufnahmebereitschaft aktiv ist. Beim Start einer Aufnahme werden diese 500 ms vorangestellt, damit kein Wortanfang fehlt.
- Geräteliste, Standardgerät, Gerätewahl per Name, sauberes Weiterlaufen beim Wechsel oder Abziehen des Geräts (Rückfall aufs Standardgerät, Ereignis melden).
- Pegelwert (RMS, 0 bis 1) etwa 30-mal pro Sekunde als Callback.
- Funktion, um die Bereitschaft komplett zu stoppen (Mikrofon frei, kein CPU-Verbrauch).
- Beispielprogramm crates/audio/examples/record_test.rs: nimmt 10 Sekunden auf und speichert WAV unter testdata/private/record_test.wav.
Tests ohne Hardware: Ringpuffer-Logik, Resampling (Sinus-Testsignal), Voranstellen der 500 ms.
```

**Du prüfst:** Den Beispielbefehl ausführen, direkt beim Start sprechen, WAV anhören. Der erste Laut muss vollständig zu hören sein.

#### Schritt 1.2 · Modell-Manager

```
Lies AGENTS.md.
Aufgabe: crates/models.
- Manifest crates/models/manifest.toml mit Einträgen: id, Anzeigename, Sprachen, Typ (asr oder vad), Download-URL(s), Archivformat, enthaltene Dateien, SHA256, Größe, Lizenz, Namensnennung.
- Einträge (recherchiere die offiziellen Downloadquellen selbst und trage echte SHA256-Werte ein, indem du die Dateien einmal lädst und prüfst):
  1. parakeet-v3-int8: sherpa-onnx-Export von nvidia/parakeet-tdt-0.6b-v3 aus den offiziellen sherpa-onnx-Releases (asr-models), Lizenz CC-BY-4.0.
  2. parakeet-primeline-de-int8: Hugging Face flozen1981/parakeet-primeline-onnx (deutscher Fine-Tune von primeline, sherpa-onnx-kompatibel), Lizenz CC-BY-4.0.
  3. whisper-large-v3-turbo: offizieller sherpa-onnx-Export, falls vorhanden; sonst Eintrag weglassen und in docs/decisions/ begründen.
  4. silero-vad: silero_vad.onnx aus den offiziellen sherpa-onnx-Releases.
- Download mit Fortschritt, Fortsetzen nach Abbruch, SHA256-Prüfung, Entpacken (tar.bz2), Ablage unter %LOCALAPPDATA%\QuatschMitSosse\models\<id>. Bei falscher Prüfsumme wird alles gelöscht.
- Funktionen: list, install, remove, is_installed, path_of. Alle Netzwerkzugriffe nur in crates/models/src/net.rs, nur zu den Hosts aus dem Manifest.
- Kommandozeilenwerkzeug tools/models-cli (list, install <id>, remove <id>).
- NOTICE um alle Modelle mit Lizenz und Namensnennung ergänzen.
Tests: Prüfsummenfehler, Abbruch und Fortsetzen (lokaler Testserver), Host außerhalb der Liste wird abgelehnt.
```

**Du prüfst:** `cargo run -p models-cli -- install parakeet-v3-int8` (oder den von Codex genannten Befehl) lädt das Modell. Danach dasselbe mit `parakeet-primeline-de-int8` und `silero-vad`.

#### Schritt 1.3 · Spracherkennung und Sprachaktivität

```
Lies AGENTS.md.
Aufgabe: crates/vad und crates/stt mit sherpa-rs.
- crates/stt: Trait SpeechEngine mit transcribe(samples: &[f32], sample_rate: u32, language_hint: Option<&str>) -> Result<Transcript>. Transcript enthält Text und Dauer der Verarbeitung.
- Implementierungen: TransducerEngine für Parakeet-Modelle (nemo_transducer, greedy_search, Anzahl Threads einstellbar) und, falls im Manifest vorhanden, WhisperEngine.
- Modelle werden erst beim ersten Diktat geladen und nach einstellbarer Leerlaufzeit (Standard 5 Minuten) entladen.
- crates/vad: Silero-VAD über sherpa-rs. Funktion, die aus einer Aufnahme die Sprachabschnitte liefert, und Funktion has_speech.
- Lange Aufnahmen werden an Sprachpausen in Stücke von höchstens 20 Sekunden geteilt und nacheinander erkannt.
- Beispielprogramm crates/stt/examples/transcribe_file.rs <wav> <modell-id>.
Tests: Unit-Tests für Aufteilung und Leerlauf-Entladen ohne Modell. Integrationstests nur mit installierten Modellen (#[ignore], mit Hinweis im README, wie man sie startet).
```

**Du prüfst:** `record_test.wav` aus Schritt 1.1 mit beiden Parakeet-Modellen transkribieren lassen. Der deutsche Text sollte erkennbar stimmen.

#### Schritt 1.4 · Testaufnahmen erstellen

```
Lies AGENTS.md.
Aufgabe: tools/corpus-recorder und Satzliste.
- Erzeuge testdata/private/sentences.txt mit 40 deutschen Sätzen, 10 bis 40 Sekunden Sprechdauer, verteilt auf: 15 KI-Prompts mit englischen Fachwörtern (z. B. Codex, Commit, Pull Request, Affiliate-Link, Feed), 8 Alltagsnachrichten, 6 E-Mails im Sie-Ton, 5 Sätze mit Zahlen, Datum, Uhrzeit, Prozent und Euro, 6 Sätze mit Selbstkorrekturen und Füllwörtern ("äh", "nein, ich meine").
- Kommandozeilenprogramm: zeigt Satz für Satz, Enter startet und stoppt die Aufnahme (mit 500-ms-Vorlauf aus crates/audio), speichert testdata/private/corpus/NNN.wav und NNN.txt (Referenztext = Satz). Taste W wiederholt, Taste S überspringt.
- Zusätzlich 10 Aufnahmen mit völliger Stille bzw. nur Tastaturgeräuschen (Referenz leer), mit eigener Ansage im Programm.
```

**Du:** Das Programm starten und alle Sätze einsprechen, so wie du sonst diktierst. Dauer etwa 30 Minuten. Sätze, die du anders sagst, in der .txt-Datei an deine Formulierung anpassen.

#### Schritt 1.5 · Benchmark und Modellentscheidung

```
Lies AGENTS.md.
Aufgabe: tools/bench.
- Liest alle Paare NNN.wav/NNN.txt aus testdata/private/corpus.
- Für jedes installierte ASR-Modell: Wortfehlerrate (Vergleich ohne Groß-/Kleinschreibung und Satzzeichen, Umlaute und ß bleiben), Zeit vom Ende der Aufnahme bis zum Text, Echtzeitfaktor, Spitzen-RAM, Zahl der Phantomtexte bei den Stille-Aufnahmen.
- Bericht als Markdown unter docs/benchmarks/asr-JJJJ-MM-TT.md, ohne die Satztexte (nur Nummern), damit nichts Privates veröffentlicht wird.
- Schreibe docs/decisions/0001-standardmodell.md: empfohlenes Standardmodell mit Begründung und Rückfallmodell. Kriterium: niedrigste Wortfehlerrate, solange die Zeit für 10 Sekunden Audio unter 1,5 Sekunden liegt.
Führe den Benchmark aus.
```

**Du prüfst:** Bericht lesen. Wenn dir die Entscheidung passt, weiter. Wenn nicht, schreib in Codex: „Nimm Modell X als Standard und passe 0001 an.“

#### Schritt 1.6 · Schutz vor erfundenen Sätzen und abgeschnittenen Anfängen

```
Lies AGENTS.md und docs/decisions/0001-standardmodell.md.
Aufgabe: Nachfilter in crates/stt.
- Aufnahmen ohne Sprache laut VAD werden gar nicht erkannt (leeres Ergebnis).
- Sperrliste crates/stt/phantoms.txt mit typischen Phantomsätzen (z. B. Untertitel-Floskeln, "Vielen Dank fürs Zuschauen"). Treffer werden nur verworfen, wenn sie den gesamten Text bilden.
- Test mit allen Stille-Aufnahmen des Korpus (Ergebnis muss leer sein) und mit 20 Sätzen, ob das erste Wort erhalten bleibt. Ergebnis im Benchmark-Bericht ergänzen.
```

**Du prüfst:** Im Bericht steht: Stille ergibt 0 Phantomtexte, erstes Wort in 20 von 20 Fällen erhalten.

### Phase 2 · Hotkey, Einfügen, Overlay (Ergebnis: v0.1)

#### Schritt 2.1 · Globaler Hotkey

```
Lies AGENTS.md.
Aufgabe: crates/hotkey.
- Globaler Tastatur-Hook (WH_KEYBOARD_LL) auf eigenem Thread über die Crate windows.
- Zwei Modi: Halten zum Sprechen und Umschalten (einmal drücken startet, nochmal drücken stoppt). Zusätzlich: Esc während der Aufnahme bricht ab, ohne einzufügen.
- Standard-Hotkey: Strg+Umschalt+Leertaste. Achtung deutsches Layout: Strg+Alt entspricht AltGr, solche Kombinationen nicht als Standard verwenden.
- Der Hotkey wird an die Ziel-App nicht weitergereicht.
- Konfliktprüfung mit einer Liste bekannter Windows- und Office-Kürzel (Datei crates/hotkey/conflicts.toml) und Warnung bei Kombinationen nur aus Sondertasten (wie Strg+Win).
- Erkennung, ob das Vordergrundfenster mit höheren Rechten läuft (Integritätsstufe des Prozesses). Dann Ereignis "ElevatedTarget" melden, weil Windows dort Hook und Eingaben blockiert.
- Zweiter konfigurierbarer Hotkey "Letzten Text erneut einfügen" (Standard frei wählbar, konfliktfrei, in docs/decisions/ notieren).
Tests: Zustandsmaschine für beide Modi und Esc ohne echte Tastatur, Konfliktprüfung, Parser für Hotkey-Texte wie "Strg+Umschalt+Leertaste".
```

**Du prüfst:** Codex nennt ein Beispielprogramm. Starten, Hotkey drücken und halten: In der Konsole erscheinen „Start“ und „Stopp“.

#### Schritt 2.2 · Text einfügen

```
Lies AGENTS.md.
Aufgabe: crates/inject.
Reihenfolge der Einfügewege:
1. UI Automation: Wenn das fokussierte Element ValuePattern oder TextPattern mit Einfügeposition unterstützt und nicht schreibgeschützt ist, Text an der Cursorposition einsetzen.
2. SendInput mit KEYEVENTF_UNICODE, Zeichen für Zeichen in Blöcken, layoutunabhängig. Zeilenumbrüche als Enter. Abbruch, wenn sich das Vordergrundfenster ändert.
3. Zwischenablage: alle Formate der aktuellen Zwischenablage sichern, Text setzen mit den Formaten ExcludeClipboardContentFromMonitorProcessing, CanIncludeInClipboardHistory = 0 und CanUploadToCloudClipboard = 0, Strg+V senden, warten, ursprünglichen Inhalt vollständig wiederherstellen.
- Die Wahl kann pro App (Prozessname) erzwungen werden, über eine Tabelle in den Einstellungen (vorerst Konfigurationsdatei).
- Rückgabe: welcher Weg funktioniert hat, oder Fehler.
- Lange Texte (über 2000 Zeichen) gehen direkt über die Zwischenablage, weil Tastatureingabe dann zu langsam ist.
Tests: Sicherung und Wiederherstellung der Zwischenablage mit Text, HTML und Bild; Zerlegung in Unicode-Eingaben mit Umlauten, ß, Emoji und Zeilenumbrüchen.
```

**Du prüfst:** Codex nennt ein Beispielprogramm, das nach 3 Sekunden „Grüße aus Überlingen – 3,5 %“ einfügt. In Editor, Browser und Word testen. Die vorher kopierte Zwischenablage muss danach unverändert sein.

#### Schritt 2.3 · Overlay

```
Lies AGENTS.md.
Aufgabe: Overlay-Fenster der App fertigstellen.
- Kleine Leiste (etwa 160×36 px) mit Zuständen Bereit (nur ein dezenter Punkt oder ausgeblendet, einstellbar), Hört zu (Pegelbalken), Verarbeitet, Fügt ein, Fehler (Text plus Knopf "Kopieren").
- Frei verschiebbar per Ziehen; Position pro Monitor gespeichert. Liegt die gespeicherte Position außerhalb aller Monitore, Standardposition unten mittig.
- Option "In Bildschirmaufnahmen verbergen" über SetWindowDisplayAffinity mit WDA_EXCLUDEFROMCAPTURE.
- Das Overlay nimmt nie den Fokus (WS_EX_NOACTIVATE), sonst landet der Text im Overlay statt in der Ziel-App.
- Hinweis-Zustand "Ziel-Fenster läuft als Administrator" mit kurzer Erklärung.
- Deutsche Texte über die Übersetzungsdateien, helles und dunkles Design nach Windows-Einstellung.
Tests: Positionskorrektur, Zustandswechsel (Komponententests in vitest).
```

**Du prüfst:** App im Entwicklungsmodus starten, Overlay verschieben, App neu starten: Position bleibt.

#### Schritt 2.4 · Alles verbinden: das erste echte Diktat

```
Lies AGENTS.md und docs/decisions/.
Aufgabe: Diktat-Ablauf in app/src-tauri verbinden.
- Hotkey → audio (mit Vorlauf) → vad → stt (Standardmodell aus 0001) → inject → Overlay-Zustände.
- Der letzte erkannte Text wird im Speicher gehalten (Verlauf folgt in Phase 4) und ist per zweitem Hotkey und im Tray-Menü erneut einfügbar.
- Ist kein Modell installiert, öffnet sich beim ersten Hotkey ein kleines Fenster mit Download-Knopf (Modell-Manager) und Fortschritt.
- Die Mikrofon-Bereitschaft (Ringpuffer) läuft nur, solange die App aktiv ist, und stoppt nach 10 Minuten ohne Diktat; der nächste Tastendruck startet sie sofort wieder. Begründe in docs/decisions/, falls dabei der Vorlauf beim ersten Diktat fehlt, und schlage eine Lösung vor.
- Fehler landen nie stumm: Overlay zeigt den Fehler, der Text bleibt kopierbar.
- Lege docs/handbuch.md an: Bedienung in 10 Zeilen.
Miss danach mit scripts/measure-idle.ps1 und nenne die Werte.
```

**Du prüfst:** In einem Editor den Hotkey halten, einen Satz sprechen, loslassen. Der Text erscheint. Das ist der Moment, ab dem du die App testweise im Alltag nutzen kannst.

#### Schritt 2.5 · Einfüge-Test in 15 Programmen

```
Lies AGENTS.md.
Aufgabe: Lege tests/inject-matrix.md an: Tabelle mit den Programmen Editor (Notepad), Word, Outlook, Chrome, Edge, Firefox, VS Code, Cursor, Windows Terminal, alte Eingabeaufforderung (conhost), PowerShell als Administrator, Remote-Desktop-Fenster, ChatGPT-Desktop-App, Codex-Eingabe, WhatsApp-Desktop; Spalten: Weg (UIA, Unicode, Zwischenablage), Umlaute ok, Zwischenablage unverändert, Bemerkung. Ergänze eine Anleitung, wie ich jeden Test durchführe (Testsatz: "Äpfel, Öl & Übermaß – 3,5 % für 12,99 €").
Dann warte. Ich trage die Ergebnisse ein und melde mich.
```

**Du:** Die Tabelle durchgehen und Ergebnisse eintragen. Danach in Codex: „Ich habe tests/inject-matrix.md ausgefüllt. Behebe alle Fehler, erzwinge wo nötig einen Einfügeweg pro App und dokumentiere echte Grenzen.“

#### Schritt 2.6 · Töne, Autostart, Lautstärke, Grundeinstellungen

```
Lies AGENTS.md.
Aufgabe:
- Kurzer Start- und Endton (eigene, lizenzfreie Töne erzeugen), abschaltbar.
- Während der Aufnahme Systemlautstärke anderer Apps auf einen einstellbaren Wert senken (Standard 20 %) und danach wiederherstellen. Nach einem Absturz beim nächsten Start wiederherstellen.
- Autostart für den aktuellen Benutzer, ein- und ausschaltbar.
- Einstellungsfenster, Seite "Allgemein": Hotkeys mit Aufzeichnen-Feld und Konfliktwarnung, Modus (Halten/Umschalten), Mikrofon mit Pegelanzeige und Test, Sprachmodell (installiert/herunterladen/entfernen), Töne, Autostart, Overlay-Optionen.
- Einstellungen als JSON unter %LOCALAPPDATA%\QuatschMitSosse\data\settings.json, mit Schema-Version.
Tests für Lautstärke-Wiederherstellung und Einstellungs-Migration.
```

**Du prüfst:** Alle Einstellungen ändern, App neu starten, Werte sind erhalten. Danach Standard-Prompt **R** und einen Release-Tag v0.1.0 mit Standard-Prompt **C** setzen lassen.

### Phase 3 · Deutsche Bereinigung, Wörterbuch, Snippets

#### Schritt 3.1 · Regelwerk, Füllwörter, Sprachbefehle

```
Lies AGENTS.md.
Aufgabe: crates/textfx, Teil 1.
- Pipeline aus einzelnen Regeln mit Namen, jede einzeln ein- und ausschaltbar. Reihenfolge: Sprachbefehle → Wörterbuch → Füllwörter → Selbstkorrekturen → Zahlen → Zeichensetzung und Großschreibung → Snippets → Stil.
- Golden-Tests: Ordner crates/textfx/tests/golden/ mit TOML-Dateien (Eingabe, aktive Regeln, erwartete Ausgabe) und einem Test, der alle Dateien ausführt.
- Regel Füllwörter: entfernt nur "äh", "ähm", "öhm", "hm", "ehm" samt überflüssiger Kommas und Leerzeichen. "also", "halt", "quasi" nur in optionaler Stufe "stark".
- Regel Sprachbefehle (Standard aus, einschaltbar): "neue Zeile", "neuer Absatz", "Komma", "Punkt", "Fragezeichen", "Ausrufezeichen", "Doppelpunkt", "Anführungszeichen auf/zu" (deutsche „ “), "Klammer auf/zu". Nur wenn das Wort allein steht oder am Satzende, damit "der Punkt ist" nicht ersetzt wird.
- Regel Schutz: Eigennamen aus dem Wörterbuch und alle Zahlen werden von keiner Regel verändert.
- Mindestens 80 Golden-Tests für diese Regeln.
```

**Du prüfst:** Codex zeigt einige Beispiele vorher/nachher. Sie sollen natürlich wirken.

#### Schritt 3.2 · Selbstkorrekturen, Zahlen, Datum

```
Lies AGENTS.md.
Aufgabe: crates/textfx, Teil 2.
- Regel Selbstkorrektur: erkennt eindeutige Muster wie "X, nein, ich meine Y", "nicht X, sondern Y" (nur wenn klar eine Korrektur gemeint ist), "X, äh, Y", "streich das", "Kommando zurück". Ersetzt nur das Satzglied direkt davor. Unsichere Fälle bleiben unverändert und werden als "unsicher" markiert (für die Änderungsansicht).
- Regel Zahlen: Zahlwörter bis zwölf bleiben Wörter, ab 13 Ziffern (einstellbar). "drei Komma fünf Prozent" → "3,5 %", "zwölf Euro neunundneunzig" → "12,99 €", "halb drei" bleibt, "vierzehn Uhr dreißig" → "14:30 Uhr", "dritter März" → "3. März", "zweitausendsechsundzwanzig" → "2026".
- Regel Zeichensetzung: nur ergänzen, was der Erkenner sicher vergessen hat (Satzanfang groß, Punkt am Ende). Keine Komma-Regeln raten.
- Mindestens 80 weitere Golden-Tests. Führe danach den Benchmark aus Schritt 1.5 zusätzlich mit Bereinigung aus und ergänze den Bericht (Vergleich Rohtext gegen bereinigten Text).
```

**Du prüfst:** Im Bericht ist die Fehlerrate mit Bereinigung nicht schlechter als ohne.

#### Schritt 3.3 · Datenbank und Wörterbuch

```
Lies AGENTS.md.
Aufgabe: crates/store und Wörterbuch.
- SQLite unter %LOCALAPPDATA%\QuatschMitSosse\data\quatsch-mit-sosse.db über rusqlite, versionierte Migrationen, automatische Sicherungskopie vor jeder Migration.
- Tabellen dictionary (term, replacement optional, aliases als Liste gesprochener Varianten, source manual/auto, created_at) und snippets (trigger, text, created_at).
- Wörterbuch-Regel in textfx: ersetzt gesprochene Varianten durch den richtigen Begriff (z. B. "kodex" → "Codex"), Groß-/Kleinschreibung des Begriffs bleibt.
- Prüfe, ob sherpa-onnx für das gewählte Modell Hotwords (kontextuelle Verstärkung, modified_beam_search) unterstützt. Wenn ja: Wörterbuch-Begriffe als Hotwords übergeben und im Benchmark messen. Wenn nein: in docs/decisions/ festhalten.
- Einstellungsseite "Wörterbuch": Liste mit Suche, Hinzufügen, Bearbeiten, Löschen, Import/Export als CSV.
- Starte das Wörterbuch mit diesen Einträgen: Codex, Claude, ChatGPT, Wispr Flow, Denis, Anton-Mall-Stadion, BGV, Lia, Saatci, Baar.
```

**Du prüfst:** „Codex“ diktieren, es wird richtig geschrieben. Eigene Begriffe ergänzen.

#### Schritt 3.4 · Snippets

```
Lies AGENTS.md.
Aufgabe: Snippets.
- Wenn ein diktierter Text ganz oder als eigener Satz einem Auslöser entspricht (unscharf: Groß-/Kleinschreibung und Satzzeichen egal), wird der gespeicherte Text eingesetzt.
- Platzhalter: {datum}, {uhrzeit}, {zwischenablage}, {cursor} (Cursor nach dem Einfügen an diese Stelle setzen, sofern technisch möglich, sonst ignorieren).
- Mehrzeilige Texte erlaubt.
- Einstellungsseite "Snippets" wie beim Wörterbuch, mit Import/Export.
- Vorbelegt: "meine E-Mail-Adresse" (Text leer lassen, ich trage ihn ein) und "Gedanken ordnen Prompt" mit dem Text: "Ordne diese unstrukturierten Gedanken zu einer klaren, gut lesbaren Fassung, ohne Inhalte hinzuzufügen oder wegzulassen."
Golden-Tests für Auslöser-Erkennung und Platzhalter.
```

**Du prüfst:** „Gedanken ordnen Prompt“ diktieren, der Text erscheint.

#### Schritt 3.5 · Änderungsansicht und Original zurückholen

```
Lies AGENTS.md.
Aufgabe:
- Für jedes Diktat Rohtext und Endtext speichern (im Speicher; dauerhaft ab Phase 4).
- Hotkey "Änderungen anzeigen" (konfliktfrei wählen, dokumentieren): kleines Fenster nahe dem Overlay mit Vorher/Nachher-Vergleich, Änderungen farbig, "unsicher" markierte Stellen gelb, Knöpfe "Original einfügen" und "Original kopieren".
- Fenster schließt mit Esc und nimmt der Ziel-App den Fokus nur so kurz wie nötig; danach Fokus zurück an das vorherige Fenster.
Komponententests für die Vergleichsdarstellung.
```

**Du prüfst:** Einen Satz mit „äh“ diktieren, Hotkey drücken, die Änderung ist sichtbar. Danach Standard-Prompt **R**.

### Phase 4 · Hauptfenster, Verlauf, Scratchpad, Sicherung (Ergebnis: v0.5)

#### Schritt 4.1 · Hauptfenster und Navigation

```
Lies AGENTS.md.
Aufgabe: Das Einstellungsfenster wird zum Hauptfenster, aufgebaut wie bei Wispr Flow, aber ruhig und ohne Werbebanner.
- Linke Leiste: Start, Wörterbuch, Snippets, Stil, Transforms, Scratchpad, Statistik, unten Einstellungen und Hilfe. Noch nicht gebaute Seiten zeigen "Kommt in einer späteren Version".
- Einstellungen mit Unterseiten: Allgemein (aus Schritt 2.6), System (Autostart, Overlay, Töne, Lautstärke), Datenschutz, Über (Version, Lizenzen aus NOTICE).
- Design: hell, dunkel, wie Windows (Standard). Schrift Segoe UI Variable, viel Weißraum, dezente Akzentfarbe.
- Tastaturbedienung vollständig, sichtbarer Fokus, Beschriftungen für Screenreader.
- Das Fenster wird beim Schließen zerstört (Speicher frei), die App läuft im Tray weiter.
Komponententests für Navigation und Designwechsel.
```

**Du prüfst:** Durch alle Seiten klicken, Dunkelmodus testen, Fenster schließen: App läuft im Tray weiter.

#### Schritt 4.2 · Verlauf

```
Lies AGENTS.md.
Aufgabe: Verlauf auf der Startseite.
- Tabelle dictations in crates/store: Zeit, App (Prozessname), Rohtext, Endtext, Sprache, Dauer, Modell, optional Pfad zur Audiodatei. Volltextsuche über FTS5.
- Startseite: Diktate nach Tagen gruppiert (Heute, Gestern, Datum), je Eintrag Uhrzeit, Text, Knöpfe Kopieren, Erneut einfügen, Abspielen (nur wenn Audio gespeichert), Löschen. Suchfeld oben. Rechts eine kleine Karte mit Wörtern gesamt und Wörtern pro Minute.
- Text im Verlauf bearbeitbar. Ändert der Nutzer ein einzelnes Wort, erscheint der Vorschlag "Zum Wörterbuch hinzufügen?".
- Einstellungen unter Datenschutz: Verlauf speichern (für immer, 30 Tage, 7 Tage, 24 Stunden, gar nicht), Audio speichern (Standard aus), Knopf "Alle Daten löschen" mit Bestätigung.
- Automatisches Löschen alter Einträge beim Start und einmal täglich.
Tests für Aufbewahrungsregeln, Suche mit Umlauten, Löschen.
```

**Du prüfst:** Drei Diktate machen, im Verlauf suchen, eins bearbeiten, Wörterbuch-Vorschlag annehmen.

#### Schritt 4.3 · Scratchpad

```
Lies AGENTS.md.
Aufgabe: Scratchpad.
- Eigener Hotkey (konfliktfrei, dokumentieren) öffnet ein kleines Notizfenster mit Fokus im Textfeld. Diktieren dort fügt direkt ins Notizfeld ein.
- Verhalten einstellbar: letzte Notiz fortsetzen (Standard) oder neue Notiz.
- Notizen in SQLite, Seite "Scratchpad" im Hauptfenster mit Liste, Suche, Kopieren, Löschen, Export als Markdown.
- Automatisches Speichern während des Tippens.
```

**Du prüfst:** Hotkey drücken, diktieren, Fenster schließen, wieder öffnen: Text ist noch da.

#### Schritt 4.4 · Sicherung, Export, Zurücksetzen

```
Lies AGENTS.md.
Aufgabe:
- Export aller Daten (Einstellungen, Wörterbuch, Snippets, Verlauf, Notizen) als ZIP mit JSON-Dateien; Import mit Vorschau, was ersetzt wird.
- Zurücksetzen der App nur mit Bestätigung und nach automatischem Export in %LOCALAPPDATA%\QuatschMitSosse\backups.
- Wöchentliche automatische Sicherung, die letzten 4 bleiben erhalten (abschaltbar).
Tests: Export und Import ergeben identische Daten; Zurücksetzen ohne Sicherung ist nicht möglich.
Miss danach den Leerlauf mit scripts/measure-idle.ps1 und vergleiche mit den Zielen.
```

**Du prüfst:** Export erstellen und Datei öffnen. Danach Standard-Prompt **R** und Tag v0.5.0 mit Standard-Prompt **C**.

### Phase 5 · Stil und Kontext

#### Schritt 5.1 · Kontext der aktiven App (lokal)

```
Lies AGENTS.md.
Aufgabe: crates/context.
- Liefert für das Vordergrundfenster: Prozessname, Fenstertitel, Steuerelementtyp des Fokus (über UI Automation), ob es ein Passwortfeld ist, ob es schreibgeschützt ist.
- Optional (Standard aus, in Datenschutz einschaltbar): bis zu 200 Zeichen Text vor dem Cursor und die Domain der aktiven Browser-Registerkarte (über UI Automation der Adressleiste von Chrome, Edge, Firefox). Nur im Speicher, nie gespeichert, nie gesendet, außer der Nutzer aktiviert das ausdrücklich für einen KI-Anbieter.
- In Passwortfelder wird nie eingefügt; das Overlay zeigt einen Hinweis.
- Keine Screenshots, keine Bildschirmaufnahme. scripts/privacy-guard.ps1 muss weiter grün sein.
Tests mit Mock-Daten; ein Beispielprogramm, das alle 2 Sekunden den Kontext ausgibt.
```

**Du prüfst:** Beispielprogramm starten, zwischen Apps wechseln. Prozessname und Feldtyp stimmen.

#### Schritt 5.2 · App-Profile

```
Lies AGENTS.md.
Aufgabe: Profile Privat, Arbeit, E-Mail, Sonstiges.
- Zuordnung über Prozessname und optional Browser-Domain. Vorbelegung: WhatsApp, Telegram, Signal, Discord, Instagram → Privat; Outlook, Thunderbird, mail.google.com, outlook.live.com → E-Mail; Teams, Slack → Arbeit; alles andere → Sonstiges.
- Seite "Stil": oben Registerkarten für die vier Profile, darunter die Zuordnungsliste mit Hinzufügen per "aktive App übernehmen" (Countdown von 3 Sekunden, dann Vordergrund-App erfassen).
- Profil wird pro Diktat ermittelt und im Verlauf gespeichert.
Tests für die Zuordnungslogik.
```

**Du prüfst:** WhatsApp-Desktop öffnen, diktieren: Im Verlauf steht Profil „Privat“.

#### Schritt 5.3 · Deutsche Stilstufen

```
Lies AGENTS.md.
Aufgabe: Stil-Regel in crates/textfx und Oberfläche.
- Drei Stufen pro Profil:
  Formell: normale Groß- und Kleinschreibung, vollständige Satzzeichen.
  Locker: normale Groß- und Kleinschreibung, kein Punkt am Ende einzelner Sätze, Ausrufe- und Fragezeichen bleiben.
  Sehr locker: alles klein, kein Punkt am Ende, Fragezeichen bleiben.
- Anrede-Schalter pro Profil: bei "Sie" werden Sie, Ihnen, Ihr, Ihre in der Anredeform großgeschrieben (nur eindeutige Fälle). Echte Umwandlung Du→Sie kommt als KI-Transform in Phase 6.
- Standard: Privat = Locker, Arbeit = Formell, E-Mail = Formell mit Sie, Sonstiges = Formell. KI-Prompts gehören zu Sonstiges.
- Oberfläche wie die Stil-Seite bei Wispr: drei Karten mit Beispielnachricht, die sich live an die Einstellung anpasst.
Mindestens 40 Golden-Tests.
```

**Du prüfst:** In WhatsApp und Outlook denselben Satz diktieren: Der Stil unterscheidet sich passend. Danach Standard-Prompt **R**.

### Phase 6 · KI-Anbieter und Transforms

#### Schritt 6.1 · Anbieter-Schnittstelle

```
Lies AGENTS.md.
Aufgabe: crates/llm, Grundgerüst.
- Trait TextProvider mit transform(instructions: &str, input: &str) -> Stream von Textstücken, plus Status (bereit, nicht angemeldet, offline, Limit erreicht, Fehler mit deutscher Meldung).
- Implementierungen vorerst: NoneProvider (Standard, liefert "KI ist ausgeschaltet") und MockProvider für Tests.
- Alle Netzwerkzugriffe ausschließlich in crates/llm/src/net.rs mit fester Host-Liste: auth.openai.com, api.openai.com, 127.0.0.1.
- Einwilligung: Vor dem ersten Aufruf eines Cloud-Anbieters einmalig ein Dialog "Der markierte bzw. diktierte Text wird an OpenAI gesendet. Audio wird nie gesendet." Ohne Zustimmung kein Aufruf.
- Einstellungsseite "KI": Anbieter wählen (Aus, Lokal, ChatGPT), Status, Einwilligung widerrufen.
Tests mit MockProvider.
```

**Du prüfst:** Seite „KI“ existiert, Standard ist „Aus“.

#### Schritt 6.2 · Anmeldung mit dem ChatGPT-Konto

```
Lies AGENTS.md. Lies dann diese offiziellen Seiten vollständig und halte dich exakt daran:
https://developers.openai.com/siwc/token-sharing-open-source
https://developers.openai.com/siwc/token-sharing-open-source/sign-in
https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions
https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery
https://developers.openai.com/siwc/ui-ux-guidelines
Aufgabe: Anmeldung in crates/llm (Modul chatgpt_auth) und Oberfläche.
Zusammenfassung des Ablaufs (Stand 17.11.2026, die Seiten haben Vorrang):
- Einmal pro Rechner eine Host-ID urn:uuid:<UUIDv4> erzeugen und dauerhaft speichern (ext_agent_host_id).
- Erstanmeldung: Systembrowser öffnen mit https://auth.openai.com/api/accounts/authorize und client_id=dynamic_agent_client, agent_name_hint=Quatsch mit Soße (URL-kodiert), ext_agent_host_id, response_type=code, redirect_uri=http://127.0.0.1:<freier Port, bevorzugt 1455>/auth/callback (nie localhost), scope="openid profile email offline_access resource.invoke chatgpt.tokens.use.direct", resource=https://api.openai.com/v1, frischer state und nonce, PKCE S256.
- Rückruf: state prüfen, error=access_denied sauber behandeln, ausgegebene client_id speichern (nie dynamic_agent_client speichern).
- Code tauschen per POST an https://auth.openai.com/api/accounts/oauth/token (authorization_code, client_id, code, code_verifier, gleiche redirect_uri, gleiche resource), ohne Client-Secret.
- ID-Token gegen die JWKS von OpenAI prüfen: Aussteller, Zielgruppe = ausgegebene client_id, Ablauf, nonce. Planfreigabe nur, wenn die erteilten Scopes chatgpt.tokens.use.direct enthalten.
- Spätere Anmeldungen mit gespeicherter client_id und id_token_hint.
- Tokens erneuern wie auf der Seite "Accounts and sessions" beschrieben; Zugriffstoken, Ablauf, Scopes und Refresh-Token immer gemeinsam ersetzen.
- Speicherung: im Windows-Anmeldeinformationsspeicher statt in einer Datei. Nie loggen.
- Oberfläche: Knopf nach den UI/UX-Richtlinien (Beschriftung "Continue with ChatGPT", falls die Richtlinien keine deutsche Variante vorsehen), Anzeige des angemeldeten Kontos, Abmelden. Hinweis: "Die Nutzung zählt zu deinem ChatGPT-Kontingent. Ein Limit pro App kannst du in den ChatGPT-Einstellungen festlegen."
Tests: PKCE, state-Prüfung, Speichern und Laden ohne echten Server (Mock).
```

**Du prüfst:** In der App auf den ChatGPT-Knopf klicken, im Browser anmelden und die Berechtigung bestätigen. Die App zeigt dein Konto an.

#### Schritt 6.3 · Anfragen an ChatGPT

```
Lies AGENTS.md. Lies diese offiziellen Seiten und halte dich exakt daran:
https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference
https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations
https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery
Aufgabe: ChatGPTProvider in crates/llm.
- POST https://api.openai.com/v1/responses mit Bearer-Zugriffstoken, store: false, stream: true, instructions für die Anweisung, input als Array mit dem Text. Keine nicht erlaubten Felder (laut Seite u. a. temperature, max_output_tokens, metadata, previous_response_id).
- Stream (SSE) bis response.completed verarbeiten; Abbruch durch den Nutzer möglich.
- Modellauswahl: verfügbare Modelle wie auf der Seite beschrieben abrufen; Standard ist ein schnelles Modell, in den Einstellungen änderbar.
- Abgelaufenes Token: einmal erneuern und wiederholen. Limit erreicht, keine Planfreigabe, offline: verständliche deutsche Meldung im Overlay, Originaltext bleibt erhalten.
Tests gegen einen lokalen Mock-Server mit aufgezeichneten SSE-Ereignissen.
```

**Du prüfst:** Auf der Seite „KI“ gibt es einen Knopf „Test senden“. Er liefert eine kurze Antwort.

#### Schritt 6.4 · Lokales Sprachmodell (optional, offline)

```
Lies AGENTS.md.
Aufgabe: LocalProvider.
- Recherchiere 3 aktuelle, frei lizenzierte Instruct-Modelle im GGUF-Format, die gut Deutsch können und auf einer CPU mit 16 GB RAM laufen (Größe, Lizenz, Quelle). Schreibe die Empfehlung nach docs/decisions/0002-lokales-llm.md und frage mich, welches ich will (Empfehlung zuerst).
- Nach meiner Antwort: llama.cpp-Server (offizielle Release-Binärdatei für Windows) und das Modell über den Modell-Manager mit SHA256 installieren. Server nur auf 127.0.0.1, wird bei Bedarf gestartet und nach 5 Minuten Leerlauf beendet.
- LocalProvider spricht die OpenAI-kompatible Schnittstelle des Servers an.
Test mit kurzem Prompt; Messung von Antwortzeit und RAM in docs/benchmarks/.
```

**Du:** Codex fragt nach dem Modell. Antworte „Nimm die Empfehlung“ oder nenne eins.

#### Schritt 6.5 · Transforms

```
Lies AGENTS.md.
Aufgabe: Transforms (Seite "Transforms" und Ablauf).
- Datenmodell: Name, Anweisung, Hotkey, Anbieter (Standard: der gewählte), aktiv für Auto-Anwenden je Profil.
- Mitgelieferte Transforms (in Einstellungen zurücksetzbar):
  Polieren: "Verbessere Klarheit und Prägnanz des folgenden Textes. Behalte Bedeutung, Ton, Sprache und Fachbegriffe bei. Gib nur den überarbeiteten Text aus."
  Prompt bauen: "Forme den folgenden diktierten Text in einen klaren Prompt für einen KI-Assistenten um: Ziel, Kontext, Anforderungen als Liste, Abnahmekriterien, Einschränkungen. Erfinde nichts dazu. Fehlende Angaben stehen als offene Fragen am Ende. Gib nur den Prompt aus."
  Gedanken ordnen: "Ordne diese unstrukturierten Gedanken zu einer klaren, gut lesbaren Fassung, ohne Inhalte hinzuzufügen oder wegzulassen. Gib nur den Text aus."
  Du zu Sie: "Schreibe den Text von der Du-Form in die Sie-Form um. Ändere sonst nichts. Gib nur den Text aus."
  Zusammenfassen: "Fasse den Text in 3 bis 5 Stichpunkten zusammen. Gib nur die Stichpunkte aus."
- Anwendung: per Hotkey auf das letzte Diktat (ersetzt den gerade eingefügten Text nur, wenn sicher möglich, sonst neu einfügen) oder auf markierten Text (Markierung per Strg+C mit vollständiger Wiederherstellung der Zwischenablage holen).
- Hotkeys Standard Win+Alt+1 bis 5, Konfliktprüfung aus crates/hotkey.
- Auto-Anwenden nach jedem Diktat pro Profil (Standard aus).
- Ergebnis immer in der Änderungsansicht aus Schritt 3.5 prüfbar, Original jederzeit zurück.
- Ist der Anbieter Aus, zeigen die Transforms einen Hinweis statt eines Fehlers.
Tests mit MockProvider.
```

**Du prüfst:** Einen wirren Gedanken diktieren, Win+Alt+2 drücken: Ein sauberer Prompt erscheint. Danach Standard-Prompt **R**.

### Phase 7 · Vibe-Coding (Ergebnis: v0.8)

#### Schritt 7.1 · Code-Sprechweise

```
Lies AGENTS.md.
Aufgabe: Profil "Code" und Regel in crates/textfx.
- Neues Profil Code, automatisch für Code.exe, Cursor.exe, WindowsTerminal.exe, codex.exe, claude.exe, idea64.exe (erweiterbar auf der Seite Stil).
- Nur in diesem Profil: "camel case foo bar" → fooBar, "pascal case foo bar" → FooBar, "snake case foo bar" → foo_bar, "kebab case foo bar" → foo-bar, "index punkt tsx" → index.tsx, "Unterstrich" → _, "Backtick foo Backtick" → `foo`, "Klammer auf/zu", "geschweifte Klammer auf/zu", "Pfeil" → ->.
- Deutsche Sätze bleiben normal; nur diese Befehle werden umgewandelt.
- Standard-Stil im Profil Code: Formell, Sprachbefehle an.
Mindestens 50 Golden-Tests. Erweitere testdata/private/sentences-code.txt um 20 Sätze mit Bezeichnern und Dateinamen für den Korpus-Rekorder.
```

**Du:** Mit dem Korpus-Rekorder die 20 Code-Sätze einsprechen (Befehl nennt Codex).

#### Schritt 7.2 · Erweiterung für VS Code und Cursor

```
Lies AGENTS.md.
Aufgabe: tools/vscode-extension (TypeScript), lokal installierbar als .vsix in VS Code und Cursor.
- Sammelt aus dem geöffneten Projekt: Dateinamen der offenen Editoren und des Workspaces (begrenzt auf 2000), Bezeichner der Symbole der offenen Dateien (über die Symbol-API des Editors).
- Stellt diese Liste der App über eine Named Pipe (\\.\pipe\quatsch-mit-sosse-ide) bereit, nur für den aktuellen Windows-Benutzer, mit zufälligem Token aus einer Datei in %LOCALAPPDATA%\QuatschMitSosse\data.
- Die App nutzt die Liste im Profil Code als zusätzliches Wörterbuch bzw. als Hotwords (falls in 3.3 möglich) und für Datei-Tags: "Datei index punkt tsx" wird im Chat-Feld von Cursor zu @index.tsx.
- Einstellungsseite "Vibe-Coding": Status der Verbindung, Datei-Tags an/aus, Anleitung zur Installation der .vsix.
Tests für Pipe-Protokoll und Tag-Umwandlung.
```

**Du:** Die erzeugte .vsix in VS Code bzw. Cursor installieren (Erweiterungen → „Aus VSIX installieren“).

#### Schritt 7.3 · Messen und nachschärfen

```
Lies AGENTS.md.
Aufgabe: Benchmark aus Schritt 1.5 um die Code-Sätze erweitern (mit Profil Code und mit verbundener IDE-Erweiterung, falls ein Projekt offen ist). Ziel: mindestens 95 % der Bezeichner und Dateinamen korrekt. Analysiere die Fehler und verbessere Regeln oder Wörterbuch, ohne die deutschen Golden-Tests zu verschlechtern. Bericht in docs/benchmarks/.
```

**Du prüfst:** Im Bericht steht ein Wert von mindestens 95 % oder eine klare Erklärung, warum nicht. Danach Standard-Prompt **R** und Tag v0.8.0.

### Phase 8 · Statistik, Härtung, Installer (Ergebnis: v1.0)

#### Schritt 8.1 · Statistik-Seite

```
Lies AGENTS.md.
Aufgabe: Seite "Statistik", rein lokal, ohne Teilen-Funktion und ohne Ranglisten.
- Kacheln: Wörter pro Minute (Durchschnitt der letzten 30 Tage), Wörter gesamt und diesen Monat, Korrekturen durch Bereinigung und Wörterbuch, Zeitersparnis gegenüber 40 Wörtern pro Minute Tippen.
- Nutzung nach Profil (Privat, Arbeit, E-Mail, Code, Sonstiges) als Balken.
- Aktivitätskalender der letzten 6 Monate und Serie in Tagen.
- Werte aus Tabelle stats (pro Tag aggregiert), damit auch bei gelöschtem Verlauf Statistiken bleiben; abschaltbar in Datenschutz.
Tests für die Berechnungen.
```

**Du prüfst:** Seite öffnen, Zahlen wirken plausibel.

#### Schritt 8.2 · Leistung auf Zielwerte bringen

```
Lies AGENTS.md (Leistungsziele) und alle Berichte in docs/benchmarks/.
Aufgabe: Messe mit scripts/measure-idle.ps1 und tools/bench. Bringe die App auf die Ziele: Leerlauf unter 100 MB ohne geladenes Modell und ohne offenes Hauptfenster, unter 1 % CPU, Start unter 2 Sekunden, unter 1,5 Sekunden vom Loslassen bis zum Text bei 10 Sekunden Audio.
Ideen, die du prüfen sollst: Modell schon beim Drücken des Hotkeys parallel zur Aufnahme laden, WebView nur bei offenem Fenster, Overlay als leichtes natives Fenster statt WebView, Release-Profil mit LTO, Threads der Erkennung passend zur CPU.
Dokumentiere jede Änderung mit Vorher/Nachher-Werten in docs/benchmarks/. Keine Funktion darf dabei verloren gehen.
```

**Du prüfst:** Der Bericht zeigt alle Ziele erreicht oder begründet die Abweichung.

#### Schritt 8.3 · Installer und Releases

```
Lies AGENTS.md.
Aufgabe:
- Tauri-Bundler: NSIS-Installer (Produktname "Quatsch mit Soße", Datei quatsch-mit-sosse-setup.exe) für den aktuellen Benutzer (keine Administratorrechte), deutsch, Startmenü-Eintrag, sauberes Deinstallieren mit Frage "Daten behalten?".
- GitHub-Actions-Workflow release.yml: bei Tag v* Installer bauen und als GitHub-Release mit SHA256-Summen hochladen.
- Optionale Update-Prüfung (Standard aus): fragt nur die GitHub-Releases-Seite dieses Repositorys ab, nach Zustimmung. Host in die Liste von net.rs aufnehmen und in privacy-guard.ps1 berücksichtigen.
- Hinweis im README: Der Installer ist nicht signiert, Windows SmartScreen kann warnen ("Weitere Informationen" → "Trotzdem ausführen").
```

**Du prüfst:** Installer aus dem GitHub-Release herunterladen, installieren, App startet aus dem Startmenü.

#### Schritt 8.4 · Dokumentation und Lizenzen

```
Lies AGENTS.md.
Aufgabe:
- README.md auf Deutsch: Was ist Quatsch mit Soße, Funktionen, Datenschutz in 5 Sätzen, Installation, erste Schritte, Lizenzhinweise.
- docs/handbuch.md vollständig: alle Hotkeys, alle Seiten, typische Probleme (Admin-Fenster, Bluetooth-Headset, kein Mikrofon) mit Lösung.
- docs/datenschutz.md: welche Daten wo liegen, wann Netzwerk genutzt wird, wie man alles löscht.
- NOTICE vollständig: alle Rust-Crates und npm-Pakete mit Lizenz (automatisch erzeugt, z. B. cargo-about bzw. license-checker), alle Modelle mit CC-BY-Namensnennung.
- CI-Schritt mit cargo-deny: nur erlaubte Lizenzen, keine bekannten Sicherheitslücken.
```

**Du prüfst:** README auf GitHub lesen, verständlich für jemanden ohne Vorwissen.

#### Schritt 8.5 · Abschlusstest v1.0

```
Lies AGENTS.md und docs/masterplan.md (Teil E, Ziele).
Aufgabe: Abschlussprüfung. Führe alle Tests, scripts/privacy-guard.ps1, scripts/measure-idle.ps1 und tools/bench aus. Erstelle docs/release-check-v1.0.md mit einer Liste aller Ziele (erreicht/nicht erreicht, Wert) und einer Checkliste für meine manuellen Tests: Einfüge-Matrix, Flugmodus-Test (WLAN aus, Diktat funktioniert), Netzwerkmitschnitt ohne KI (keine Verbindungen), Neustart, Deinstallation. Behebe, was automatisch prüfbar ist.
```

**Du:** Die Checkliste in `docs/release-check-v1.0.md` durchgehen. Wenn alles passt: Standard-Prompt **C** mit Tag v1.0.0.

### Phase 9 · Notetaker und MCP (Ergebnis: v1.5)

Wichtig vorab: Gespräche anderer ohne deren Einwilligung aufzuzeichnen, kann in Deutschland strafbar sein (§ 201 StGB). Das ist keine Rechtsberatung. Die App ist deshalb so gebaut, dass sie nie automatisch aufnimmt und immer sichtbar anzeigt, dass sie läuft.

#### Schritt 9.1 · Meeting-Aufnahme

```
Lies AGENTS.md.
Aufgabe: crates/notes, Aufnahme.
- Zwei getrennte Spuren: Mikrofon ("Ich") und Systemaudio per WASAPI-Loopback ("Andere"), beide 16 kHz mono, als Opus- oder FLAC-Datei unter %LOCALAPPDATA%\QuatschMitSosse\data\meetings, nur solange die Notiz existiert.
- Start nur manuell: Hotkey Win+Alt+M (Konfliktprüfung), Tray-Menü oder Startknopf. Nie automatisch.
- Erkennung laufender Anrufe über aktive Audio-Sitzungen bekannter Programme (Teams, Zoom, Webex, Discord, Browser): kleine Benachrichtigung "Meeting erkannt. Notiz starten?" (abschaltbar).
- Maximale Länge einstellbar (Standard 2 Stunden) mit Warnung 5 Minuten vorher; Stopp, wenn die Audio-Sitzung des Anrufs endet (einstellbar).
- Aufnahme-Anzeige: roter Punkt im Overlay und im Tray, solange aufgenommen wird. Diese Anzeige lässt sich nicht abschalten.
Tests für Zustände, Zeitlimit und Ende-Erkennung mit simulierten Audio-Sitzungen.
```

**Du prüfst:** Ein YouTube-Video abspielen, Notiz starten, selbst sprechen, stoppen. Beide Spuren sind in der Notiz abspielbar.

#### Schritt 9.2 · Live-Transkript, Notizfenster, Einwilligung

```
Lies AGENTS.md.
Aufgabe:
- Live-Transkript: beide Spuren in Abschnitten von wenigen Sekunden mit dem Standardmodell erkennen, Zeitstempel, Sprecher "Ich" oder "Andere".
- Notizfenster mit zwei Ansichten: "Notizblock" (eigene Stichpunkte, frei tippbar oder diktierbar) und "Nebeneinander" (Notizblock plus Live-Transkript). Ansicht einstellbar.
- Einwilligung: Knopf "Hinweis einfügen", der den Text "Hinweis: Ich transkribiere dieses Gespräch lokal für meine Notizen. Bitte sag Bescheid, wenn du das nicht möchtest." (anpassbar) in das fokussierte Chatfeld einfügt. Einstellung "Bei jedem Start erinnern" (Standard an).
- Option "Notizfenster in Bildschirmfreigabe verbergen" (WDA_EXCLUDEFROMCAPTURE).
- Nach dem Ende: vollständige Neuerkennung der ganzen Aufnahme für bessere Qualität, danach Audio löschen oder behalten (Einstellung, Standard: löschen nach 7 Tagen).
```

**Du prüfst:** Testmeeting mit einem Video: Transkript läuft live mit, Hinweis lässt sich einfügen.

#### Schritt 9.3 · Sprechertrennung

```
Lies AGENTS.md.
Aufgabe: Sprechertrennung lokal.
- Die Spur "Ich" ist immer Sprecher "Ich". Nur die Spur "Andere" wird getrennt.
- Prüfe die Sprechertrennung (speaker diarization) von sherpa-onnx und die dafür nötigen Modelle (Segmentierung und Sprecher-Embedding) mit Lizenz. Modelle über den Modell-Manager mit SHA256.
- Ergebnis: Sprecher 1, Sprecher 2 usw., in der Notiz umbenennbar (Umbenennung gilt für die ganze Notiz).
- Entscheidung und Lizenzen in docs/decisions/ und NOTICE.
Miss die Qualität an 2 eigenen Testaufnahmen und dokumentiere sie.
```

**Du:** Zwei kurze Testgespräche (z. B. Podcast-Ausschnitte über die Lautsprecher) als Notiz aufnehmen.

#### Schritt 9.4 · Zusammenfassung, Ordner, Export

```
Lies AGENTS.md.
Aufgabe:
- Nach dem Meeting optional über den gewählten KI-Anbieter: Zusammenfassung, Entscheidungen, Aufgaben mit Verantwortlichen, offene Fragen. Ohne Anbieter: nur Transkript. Vor dem Senden zeigt die App, wie viele Wörter an welchen Anbieter gehen.
- "Frag dein Meeting": Eingabefeld, das Fragen zur Notiz über den Anbieter beantwortet (nur Text der Notiz als Kontext).
- Seite "Notizen" im Hauptfenster: Liste, Ordner, Volltextsuche (FTS5), Umbenennen, Löschen.
- Export als Markdown, Text und PDF. Kein Teilen über Links.
```

**Du prüfst:** Eine Notiz zusammenfassen lassen und als PDF exportieren.

#### Schritt 9.5 · Lokaler MCP-Server für Claude, ChatGPT und Codex

```
Lies AGENTS.md. Lies die aktuelle MCP-Spezifikation (modelcontextprotocol.io) und die offiziellen Seiten zur MCP-Einrichtung von Codex und Claude Desktop.
Aufgabe: crates/mcp und Programm quatsch-mit-sosse-mcp.exe.
- MCP-Server über stdio (kein Netzwerkport), liest die Datenbank nur lesend.
- Werkzeuge: list_notes, search_notes(query), get_note(id), get_transcript(id). Diktate und Verlauf sind ausgeschlossen; eine Einstellung "Diktate für MCP freigeben" (Standard aus) schaltet search_dictations frei.
- Seite "MCP" im Hauptfenster: Schalter an/aus und fertige Konfigurationsschnipsel zum Kopieren für Codex (config.toml), Claude Desktop und andere Clients, mit dem richtigen Pfad zur exe.
Tests mit einem MCP-Testclient.
```

**Du prüfst:** Schnipsel in Codex oder Claude Desktop eintragen und fragen: „Was waren die Aufgaben aus meinem letzten Meeting?“ Danach Standard-Prompt **R** und Tag v1.5.0.

## Standard-Prompts für jeden Schritt

### C · Commit (nach jedem erfolgreichen Schritt)

```
Lies AGENTS.md. Prüfe mit git status, was sich geändert hat. Führe alle Prüfungen aus AGENTS.md und scripts/privacy-guard.ps1 aus. Wenn alles grün ist: Commit mit einer kurzen deutschen Nachricht im Format "Schritt X.Y: <was>" und push. Wenn ich in dieser Nachricht einen Tag nenne (z. B. v0.1.0), setze ihn und pushe ihn. Wenn etwas rot ist: nicht committen, sondern Fehler nennen.
```

### F · Fehler beheben

Den Fehlertext oder deine Beobachtung unter den Prompt schreiben.

```
Lies AGENTS.md. Beim letzten Schritt ist folgender Fehler aufgetreten (siehe unten). Finde die Ursache, bevor du etwas änderst, und erkläre sie mir in zwei Sätzen. Behebe sie mit der kleinstmöglichen Änderung, schreibe einen Test, der den Fehler vorher gezeigt hätte, und führe alle Prüfungen aus. Ändere nichts, was nicht zum Fehler gehört.

Fehler bzw. Beobachtung:

```

### W · Weiter nach Abbruch

```
Lies AGENTS.md. Die letzte Sitzung wurde unterbrochen. Prüfe mit git status und git diff, was bereits gemacht ist, vergleiche es mit dem Schritt aus docs/masterplan.md, den ich unten nenne, und mache genau dort weiter. Nichts doppelt anlegen.

Schritt:

```

### R · Review nach jeder Phase

```
Lies AGENTS.md und docs/masterplan.md. Prüfe den Stand der gerade abgeschlossenen Phase (ich nenne sie unten) kritisch wie ein erfahrener Reviewer: Fehlen Teile aus den Prompts der Phase? Verstöße gegen die Datenschutz-Regeln? Fehlerbehandlung, Tests, Speicherlecks, blockierende Aufrufe im UI-Thread? Liste die Funde nach Schwere (hoch, mittel, niedrig). Behebe alle hohen und mittleren Funde direkt, jeweils mit Test. Niedrige als Aufgaben in docs/todo.md.

Phase:

```

### Q · Frage oder Idee zwischendurch

```
Lies AGENTS.md und docs/masterplan.md. Ich habe eine Frage bzw. Idee (unten). Beantworte sie oder bewerte die Idee in Bezug auf den Plan, aber ändere noch keinen Code. Wenn die Idee sinnvoll ist, schlage vor, in welchem Schritt sie umgesetzt wird, und formuliere einen Prompt dafür im Stil des Masterplans.

Frage oder Idee:

```

### Tipp für Claude Code statt Codex

Die Prompts funktionieren unverändert. Claude Code liest über `CLAUDE.md` dieselben Regeln aus `AGENTS.md`. Starte für jeden Schritt eine frische Sitzung mit `/clear`.

## Teil E – Ziele, Risiken, Quellen

### Ziele (Abnahmewerte)

Die Zahlen sind Zielwerte. Sie werden in Schritt 1.5, 8.2 und 8.5 auf deinem Rechner gemessen.

| Messgröße | Ziel |
| --- | --- |
| Leerlauf-RAM ohne geladenes Modell und ohne offenes Hauptfenster | unter 100 MB |
| Leerlauf-CPU | unter 1 % |
| Start bis bereit | unter 2 Sekunden |
| Loslassen bis Text bei 10 Sekunden Audio | unter 1,5 Sekunden |
| Verlorene Wortanfänge | 0 von 20 |
| Phantomtexte bei Stille | 0 von 10 |
| Einfügeerfolg in der Matrix | mindestens 99 %, Ausnahmen dokumentiert |
| Netzwerkverkehr ohne KI und ohne Downloads | keiner |
| Bezeichner im Profil Code | mindestens 95 % korrekt |
| Wortfehlerrate | wird in Schritt 1.5 aus deinen Aufnahmen festgelegt |

### Risiken und Rückfallpläne

| Risiko | Rückfall |
| --- | --- |
| Erkennung auf deiner CPU zu langsam | kleineres Modell, weniger Threads für andere Aufgaben, GPU-Beschleunigung über DirectML prüfen |
| `sherpa-rs` fehlt eine benötigte Funktion | offizielle C-Schnittstelle von sherpa-onnx direkt einbinden |
| ChatGPT-Plan-Nutzung ändert sich (Preview) | lokales Sprachmodell aus Schritt 6.4; die App bleibt ohne KI vollständig nutzbar |
| Einfügen scheitert in einem Programm | Einfügeweg pro App erzwingen, im Handbuch dokumentieren |
| Hotkey wirkt nicht in Administrator-Fenstern | Hinweis im Overlay; Quatsch mit Soße bei Bedarf selbst als Administrator starten |
| Codex macht zu große Änderungen | Standard-Prompt F, Schritt kleiner formulieren, `git restore` auf den letzten Commit |
| Modell-Lizenzen (CC-BY-4.0) | Namensnennung in NOTICE und „Über“, kein Weiterverkauf geplant |

### Quellen (geprüft am 16. und 17.11.2026)

- OpenAI: Sign in with ChatGPT – Überblick zur Plan-Nutzung: https://developers.openai.com/siwc/token-sharing-open-source
- OpenAI: Registration and sign-in: https://developers.openai.com/siwc/token-sharing-open-source/sign-in
- OpenAI: Preview limitations: https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations
- OpenAI: Codex app-server mit ChatGPT-Plan: https://developers.openai.com/siwc/token-sharing-open-source/codex-app-server
- Deutsches Parakeet als sherpa-onnx-Export: https://huggingface.co/flozen1981/parakeet-primeline-onnx
- Parakeet-v3-Export der sherpa-onnx-Releases (Verweis und Lizenz): https://huggingface.co/CreativeSkipper/MynaLuna-ASR
- Rust-Beispiel für sherpa-rs mit Parakeet v3: https://docs.rs/crate/minutes-core/0.19.0/source/src/sherpa\_engine.rs
- OpenWritr (Rust, Parakeet v3 int8 unter Windows): https://github.com/trsdn/openwritr-windows/releases/tag/v0.2.0
- Codex CLI unter Windows: https://itecsonline.com/post/how-to-install-codex-cli-on-windows-2026-guide
- Codex-Installation und /init: https://www.analyticsvidhya.com/blog/2026/08/how-to-install-codex-cli/
- AGENTS.md und CLAUDE.md gemeinsam nutzen: https://maju-tech.blogspot.com/2026/04/openai-codex-cli-install-complete-guide.html
- Wispr-Probleme, Modellvergleich und alle weiteren Quellen: siehe Flow-DE\_Informationssammlung.pdf, Abschnitt 9.
