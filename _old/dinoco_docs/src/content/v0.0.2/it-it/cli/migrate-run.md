# Come si usa?

Il comando `dinoco migrate run` esegue tutte le migrazioni in sospeso.

Viene utilizzato per allineare il database configurato con la cronologia delle migrazioni già generate.

---

## Cosa fa il comando

Quando si esegue questo comando, la CLI:

- Legge la cronologia delle migrazioni
- Verifica quali non sono ancora state applicate
- Esegue le migrazioni in sospeso in ordine

## Quando usarlo

Usa questo comando quando:

- Hai già delle migrazioni generate
- Vuoi applicare le modifiche in sospeso al database
- Stai preparando un ambiente locale o di deploy

## Prossimi passi

Se necessario, genera i modelli con:

```bash
dinoco models generate
```
