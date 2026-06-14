# Come si usa?

Il comando `dinoco migrate rollback` annulla l'ultima migrazione applicata.

È utile quando è necessario annullare la modifica più recente nel database.

---

## Cosa fa il comando

Eseguendo questo comando, la CLI tenta di:

- Identificare l'ultima migrazione applicata
- Annullare tale migrazione
- Aggiornare la cronologia delle migrazioni del database

## Parametri

[NOME]: Nome della migrazione (per annullare una specifica) (Opzionale)

## Esempio di utilizzo

```bash
dinoco migrate rollback
```

## Quando usarlo

Usa questo comando quando:

- L'ultima migrazione deve essere annullata
- Hai rilevato un problema recente nello schema
- Stai regolando il flusso di evoluzione del database in fase di sviluppo

## Precauzioni

- Non tutte le modifiche strutturali sono banali da annullare senza impatto.
- Rivedi l'effetto della migrazione prima di eseguire un rollback in ambienti importanti.

## Passi successivi

Dopo aver regolato lo schema, puoi generare una nuova migrazione:

```bash
dinoco migrate generate
```
