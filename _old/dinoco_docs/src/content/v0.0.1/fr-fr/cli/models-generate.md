# Comment utiliser ?

La commande `dinoco models generate` génère les modèles Rust à partir de la dernière migration stockée dans la base de données.

Elle est utilisée pour maintenir le code généré aligné avec la structure du schéma déjà appliquée.

---

## Ce que fait la commande

Cette commande :

- Lit la dernière migration disponible
- Reconstruit les modèles Rust générés
- Met à jour les artefacts utilisés par l'API typée de Dinoco

## Quand l'utiliser

Utilisez cette commande lorsque :

- Vous avez déjà appliqué des migrations et souhaitez régénérer le code
- Vous devez mettre à jour les modèles après des modifications structurelles
- Vous souhaitez vous assurer que le client typé reflète l'état le plus récent

## Prochaines étapes

Après la génération, vous pouvez maintenant utiliser les modèles dans votre code Rust.
