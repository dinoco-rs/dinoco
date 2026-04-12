# Comment utiliser ?

La commande `dinoco migrate generate` génère une migration à partir du schéma actuel.

Elle compare l'état actuel du schéma avec l'historique connu et crée les artefacts nécessaires pour faire évoluer la base de données.

---

## Ce que fait la commande

Cette commande :

- Lit le schéma actuel
- Génère une nouvelle migration locale
- Prépare les artefacts utilisés par Dinoco pour l'évolution de la base de données

Optionnellement, elle peut également appliquer la migration immédiatement et générer les modèles Rust.

## Paramètres

### --apply

Applique la migration générée immédiatement et génère également les modèles Rust.

Exemple :

```bash
dinoco migrate generate --apply
```

## Exemple d'utilisation sans appliquer

```bash
dinoco migrate generate
```

Ce flux est utile lorsque vous souhaitez :

- Inspecter la migration avant de l'appliquer
- Réviser les changements dans le contrôle de version
- Séparer la génération et l'exécution en différentes étapes

## Exemple d'utilisation avec application immédiate

```bash
dinoco migrate generate --apply
```

Ce flux est utile lorsque vous souhaitez :

- Mettre à jour la base de données locale rapidement
- Générer les modèles juste après la migration
- Itérer plus rapidement pendant le développement

## Prochaines étapes

Après la génération, vous pouvez :

```bash
dinoco migrate run
```

ou :

```bash
dinoco models generate
```
