# Introduction à Dinoco

Cette bibliothèque a été créée dans le but de faciliter la gestion de bases de données. Inspiré par la philosophie de [Prisma](https://www.prisma.io/), Dinoco apporte à l'écosystème [Rust](https://www.rust-lang.org/) une interface fluide, typée et extrêmement productive.

---

## L'Expérience Prisma en Rust

Dinoco est né du désir d'apporter l'agilité et le typage fort de Prisma à l'écosystème [Rust](https://www.rust-lang.org/). Nous pensons que vous ne devriez pas perdre des heures à configurer des macros complexes ou à écrire du SQL manuel pour les opérations quotidiennes courantes.

## Piliers du Projet

- **DX (Expérience développeur) avant tout :** Une API fluide et intuitive qui réduit la charge cognitive et accélère le développement.
- **Sécurité des types de bout en bout :** Nous garantissons que vos données sont toujours synchronisées avec vos définitions de types, en capturant les erreurs dès la compilation.
- **Relations simplifiées :** Oubliez les jointures manuelles exhaustives. Définissez vos relations et laissez Dinoco gérer la complexité en coulisses.
- **CLI puissante :** Gestion des migrations et génération de code intégrées pour maintenir votre flux de travail organisé et rapide.

## Où nous positionnons-nous ?

L'écosystème [Rust](https://www.rust-lang.org/) dispose d'outils puissants, mais qui exigent souvent une courbe d'apprentissage abrupte. **Dinoco** vient combler l'espace entre les options actuelles :

- **Face à [Diesel](https://diesel.rs/) :** Alors que Diesel exige une connaissance approfondie du SQL et possède un DSL (Domain Specific Language) complexe, Dinoco privilégie une syntaxe propre et axée sur la productivité, sans que vous ayez à lutter contre le compilateur pour des requêtes simples.
- **Face à [SeaORM](https://www.sea-ql.org/SeaORM/) :** SeaORM est excellent, mais son architecture basée sur des couches peut être excessivement verbeuse pour de nombreux projets. Dinoco supprime le "code répétitif" inutile, offrant la même robustesse avec beaucoup moins de lignes de code.

Si vous recherchez la performance de [Rust](https://www.rust-lang.org/) avec la fluidité que Prisma a apportée au monde Node.js, Dinoco est le bon choix pour votre prochain projet.
