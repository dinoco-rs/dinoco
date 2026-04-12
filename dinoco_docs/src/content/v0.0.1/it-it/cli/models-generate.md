# Come usare?

Il comando `dinoco models generate` genera i modelli Rust dall'ultima migrazione memorizzata nel database.

Viene utilizzato per mantenere il codice generato allineato con la struttura dello schema già applicata.

---

## Cosa fa il comando

Questo comando:

- Legge l'ultima migrazione disponibile
- Ricostruisce i modelli Rust generati
- Aggiorna gli artefatti utilizzati dall'API tipizzata di Dinoco

## Quando usare

Usa questo comando quando:

- Hai già applicato le migrazioni e desideri rigenerare il codice
- Devi aggiornare i modelli dopo modifiche strutturali
- Vuoi assicurarti che il client tipizzato rifletta lo stato più recente

## Prossimi passi

Dopo la generazione, puoi ora usare i modelli nel tuo codice Rust.
