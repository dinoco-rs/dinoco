# Comment utiliser ?

La commande `dinoco database reset` réinitialise la base de données configurée.

Elle est utile en développement lorsque vous souhaitez nettoyer complètement l'état actuel de la base de données.

---

## Ce que fait la commande

Lors de l'exécution de cette commande, la CLI tente de supprimer les objets de la base de données actuelle et de recréer un état propre pour l'environnement configuré.

Ce flux est particulièrement utile pour :

- Redémarrer un environnement local à partir de zéro
- Nettoyer les données de test
- Corriger les environnements incohérents pendant le développement

## Précautions

- Cette commande est destructive pour la base de données configurée.
- À utiliser avec prudence en dehors d'un environnement local.
- Toujours confirmer que l'URL de connexion pointe vers la bonne base de données.

## Étapes suivantes

Après la réinitialisation, vous exécutez généralement :

```bash
dinoco migrate run
```

ou :

```bash
dinoco migrate generate --apply
```
