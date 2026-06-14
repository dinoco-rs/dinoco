# Come si usa?

Il comando `dinoco migrate generate` genera una migrazione dallo schema attuale.

Confronta lo stato attuale dello schema con la cronologia conosciuta e crea gli artefatti necessari per evolvere il database.

---

## Cosa fa il comando

Questo comando:

- Legge lo schema attuale
- Genera una nuova migrazione locale
- Prepara gli artefatti usati da Dinoco per l'evoluzione del database

Opzionalmente, può anche applicare immediatamente la migrazione e generare i modelli Rust.

## Parametri

### --apply

Applica immediatamente la migrazione generata e genera anche i modelli Rust.

Esempio:

```bash
dinoco migrate generate --apply
```

## Esempio di utilizzo senza applicazione

```bash
dinoco migrate generate
```

Questo flusso è utile quando si desidera:

- Ispezionare la migrazione prima di applicarla
- Rivedere le modifiche nel controllo di versione
- Separare la generazione e l'esecuzione in fasi diverse

## Esempio di utilizzo con applicazione immediata

```bash
dinoco migrate generate --apply
```

Questo flusso è utile quando si desidera:

- Aggiornare rapidamente il database locale
- Generare i modelli subito dopo la migrazione
- Iterare più velocemente durante lo sviluppo

## Prossimi passi

Dopo la generazione, puoi:

```bash
dinoco migrate run
```

oppure:

```bash
dinoco models generate
```
