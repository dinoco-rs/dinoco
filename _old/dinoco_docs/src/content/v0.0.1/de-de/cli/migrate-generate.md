# Wie benutzt man es?

Der Befehl `dinoco migrate generate` generiert eine Migration aus dem aktuellen Schema.

Er vergleicht den aktuellen Zustand des Schemas mit der bekannten Historie und erstellt die notwendigen Artefakte, um die Datenbank zu entwickeln.

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

## Anwendungsbeispiel ohne sofortige Anwendung

```bash
dinoco migrate generate
```

Dieser Workflow ist nützlich, wenn Sie:

- Die Migration vor der Anwendung inspizieren möchten
- Die Änderungen in der Versionskontrolle überprüfen möchten
- Die Generierung und Ausführung in verschiedenen Schritten trennen möchten

## Anwendungsbeispiel mit sofortiger Anwendung

```bash
dinoco migrate generate --apply
```

Dieser Workflow ist nützlich, wenn Sie:

- Die lokale Datenbank schnell aktualisieren möchten
- Die Modelle direkt nach der Migration generieren möchten
- Während der Entwicklung schneller iterieren möchten

## Nächste Schritte

Nach der Generierung können Sie:

```bash
dinoco migrate run
```

oder:

```bash
dinoco models generate
```
