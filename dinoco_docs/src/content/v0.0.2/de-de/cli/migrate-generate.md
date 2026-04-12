# Wie wird es verwendet?

Der Befehl `dinoco migrate generate` generiert eine Migration aus dem aktuellen Schema.

Er vergleicht den aktuellen Schemazustand mit dem bekannten Verlauf und erstellt die notwendigen Artefakte zur Weiterentwicklung der Datenbank.

---

## Was der Befehl tut

Dieser Befehl:

- Liest das aktuelle Schema
- Generiert eine neue lokale Migration
- Bereitet die von Dinoco verwendeten Artefakte für die Datenbankentwicklung vor

Optional kann er die Migration auch sofort anwenden und die Rust-Modelle generieren.

## Parameter

### --apply

Wendet die generierte Migration sofort an und generiert auch die Rust-Modelle.

Beispiel:

```bash
dinoco migrate generate --apply
```

## Anwendungsbeispiel ohne Anwendung

```bash
dinoco migrate generate
```

Dieser Workflow ist nützlich, wenn Sie möchten:

- Die Migration vor der Anwendung prüfen
- Änderungen in der Versionskontrolle überprüfen
- Generierung und Ausführung in verschiedene Schritte trennen

## Anwendungsbeispiel mit sofortiger Anwendung

```bash
dinoco migrate generate --apply
```

Dieser Workflow ist nützlich, wenn Sie möchten:

- Die lokale Datenbank schnell aktualisieren
- Die Modelle direkt nach der Migration generieren
- Schneller während der Entwicklung iterieren

## Nächste Schritte

Nach der Generierung können Sie:

```bash
dinoco migrate run
```

oder:

```bash
dinoco models generate
```
