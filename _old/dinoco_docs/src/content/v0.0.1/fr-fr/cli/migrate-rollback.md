# Comment utiliser ?

La commande `dinoco migrate rollback` annule la dernière migration appliquée.

Elle est utile lorsque vous devez annuler la modification la plus récente dans la base de données.

---

## Ce que fait la commande

Lors de l'exécution de cette commande, la CLI tente de :

- Identifier la dernière migration appliquée
- Annuler cette migration
- Mettre à jour l'historique des migrations de la base de données

## Paramètres

[NOM]: Nom de la migration (pour annuler une migration spécifique) (Facultatif)

## Exemple d'utilisation

```bash
dinoco migrate rollback
```

## Quand l'utiliser

Utilisez cette commande lorsque :

- La dernière migration doit être annulée
- Vous avez détecté un problème récent dans le schéma
- Vous ajustez le flux d'évolution de la base de données en développement

## Précautions

- Toutes les modifications structurelles ne sont pas triviales à annuler sans impact.
- Examinez l'effet de la migration avant d'effectuer un rollback dans des environnements importants.

## Étapes suivantes

Après avoir ajusté le schéma, vous pouvez générer une nouvelle migration :

```bash
dinoco migrate generate
```
