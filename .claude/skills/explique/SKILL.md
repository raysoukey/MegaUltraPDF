---
name: explique
description: Explication pédagogique d'un fichier, module, fonction ou type du projet, au niveau d'un apprenant Rust ayant suivi quelques tutoriels. Utiliser quand l'utilisateur dit « /explique <chemin ou symbole> », « explique-moi ce module », « comment marche X ».
---

# Expliquer un morceau du projet

Cible : `$ARGUMENTS` (un chemin comme `crates/mega-core/src/engine.rs`, ou un symbole comme
`Document::render_page`). Sans argument, demander quoi expliquer. **Ne rien modifier.**

Lire le code visé et ses voisins immédiats (qui l'appelle, ce qu'il appelle), puis répondre en
français, dans cet ordre :

1. **Rôle en une phrase** : à quoi ça sert dans l'application, pourquoi ça existe.
2. **Les types en jeu** : un petit schéma texte des structures, enums et traits concernés,
   avec qui possède quoi (ownership) et qui emprunte quoi.
3. **Le flux** : suivre une donnée de l'entrée à la sortie, étape par étape, en citant
   `fichier:ligne` pour que l'utilisateur puisse suivre dans l'éditeur.
4. **Pourquoi ces choix** : les alternatives écartées et ce qu'elles auraient coûté
   (renvoyer aux ADR de `docs/adr/` quand il y en a un).
5. **Pièges et points d'attention** : erreurs classiques, ce qui casserait si on changeait X.
6. **Deux questions de compréhension** pour l'utilisateur, avec les réponses repliées plus bas.

Niveau : expliquer `Result`/`?`, les emprunts, les traits et les lifetimes quand ils apparaissent,
sans supposer qu'ils sont acquis ; ne pas réexpliquer `let`, `fn`, `struct` de base. Si un
concept mérite une leçon à part, proposer `/concept <nom>`.
