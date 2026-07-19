# Wie benutzt man es?

Der Befehl `dinoco models generate` generiert die Rust-Modelle aus der letzten in der Datenbank gespeicherten Migration.

Er wird verwendet, um den generierten Code mit der bereits angewendeten Schemastruktur in Einklang zu halten.

---

## Was der Befehl tut

Dieser Befehl:

- Liest die letzte verfügbare Migration
- Rekonstruiert die generierten Rust-Modelle
- Aktualisiert die von der typisierten Dinoco-API verwendeten Artefakte

## Wann zu verwenden

Verwenden Sie diesen Befehl, wenn:

- Sie bereits Migrationen angewendet haben und den Code neu generieren möchten
- Sie die Modelle nach strukturellen Änderungen aktualisieren müssen
- Sie sicherstellen möchten, dass der typisierte Client den neuesten Zustand widerspiegelt

## Nächste Schritte

Nach der Generierung können Sie die Modelle nun in Ihrem Rust-Code verwenden.
