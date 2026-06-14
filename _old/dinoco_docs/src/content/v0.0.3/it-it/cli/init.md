# Come si usa?

Il comando `dinoco init` inizializza l'ambiente Dinoco nel progetto corrente.

Prepara la base necessaria per iniziare a lavorare con schema, migrazioni e generazione di modelli.

---

## Cosa fa il comando

Eseguendo `dinoco init`, la CLI guida la configurazione iniziale dell'ambiente Dinoco.

In generale, questo flusso prepara:

- Struttura base della directory `dinoco`
- File di schema iniziali
- Configurazione del database
- Base per future migrazioni

## Quando usarlo

Usa questo comando quando:

- Stai configurando Dinoco per la prima volta nel progetto
- Vuoi creare la struttura iniziale dell'ambiente locale
- Hai bisogno di preparare il progetto per generare migrazioni e modelli

## Prossimi passi

Dopo `init`, il flusso più comune è:

```bash
dinoco migrate generate
```

oppure, se vuoi generare e applicare immediatamente:

```bash
dinoco migrate generate --apply
```
