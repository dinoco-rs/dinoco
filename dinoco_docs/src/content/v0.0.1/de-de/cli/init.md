# Wie benutzt man es?

Der Befehl `dinoco init` initialisiert die Dinoco-Umgebung im aktuellen Projekt.

Er bereitet die notwendige Grundlage vor, um mit Schemas, Migrationen und Modellgenerierung zu beginnen.

---

## Was der Befehl tut

Beim Ausführen von `dinoco init` führt die CLI durch die initiale Konfiguration der Dinoco-Umgebung.

Im Allgemeinen bereitet dieser Ablauf vor:

- Grundlegende Verzeichnisstruktur von `dinoco`
- Initiale Schema-Dateien
- Datenbankkonfiguration
- Grundlage für zukünftige Migrationen

## Wann zu verwenden

Verwenden Sie diesen Befehl, wenn:

- Sie Dinoco zum ersten Mal im Projekt einrichten
- Sie die initiale Struktur der lokalen Umgebung erstellen möchten
- Sie das Projekt für die Generierung von Migrationen und Modellen vorbereiten müssen

## Nächste Schritte

Nach `init` ist der häufigste Ablauf:

```bash
dinoco migrate generate
```

oder, wenn Sie sofort generieren und anwenden möchten:

```bash
dinoco migrate generate --apply
```
