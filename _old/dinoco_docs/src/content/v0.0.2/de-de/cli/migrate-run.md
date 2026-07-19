# Wie benutzt man es?

Der Befehl `dinoco migrate run` führt alle ausstehenden Migrationen aus.

Er wird verwendet, um die konfigurierte Datenbank mit dem bereits generierten Migrationsverlauf abzugleichen.

---

## Was der Befehl tut

Beim Ausführen dieses Befehls führt die CLI:

- Den Migrationsverlauf lesen
- Prüfen, welche noch nicht angewendet wurden
- Die ausstehenden Migrationen in der richtigen Reihenfolge ausführen

## Wann zu verwenden

Verwenden Sie diesen Befehl, wenn:

- Sie bereits Migrationen generiert haben
- Sie die ausstehenden Änderungen in der Datenbank anwenden möchten
- Sie eine lokale oder Bereitstellungsumgebung vorbereiten

## Nächste Schritte

Generieren Sie bei Bedarf die Modelle mit:

```bash
dinoco models generate
```
