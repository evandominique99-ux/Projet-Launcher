# Projet Launcher (nom temporaire)

Launcher Minecraft pour le serveur modé Projet, écrit en Rust avec une interface [Slint](https://slint.dev).

## Origine et licence

Ce logiciel est une œuvre dérivée **modifiée** de [Mistly Launcher](https://github.com/Misty-MC/Misty-Launcher) (Misty-MC). Ce n'est pas une version officielle de Misty et il n'est pas approuvé par Misty-MC.

Il est distribué sous la **Misty License 1.0** (voir [LICENSE.txt](LICENSE.txt) et [NOTICE-PROJET.md](NOTICE-PROJET.md)) : usage et modification libres, **aucune vente**, aucun paiement requis pour télécharger, installer, débloquer ou utiliser le logiciel ou l'une de ses fonctions.

## Modifications par rapport à l'original

- Nom affiché remplacé par « Projet » ; logos Misty remplacés par un fond noir temporaire.
- Mises à jour automatiques vers ce dépôt (`evandominique99-ux/Projet-Launcher`).
- Plus aucun envoi vers les serveurs de Misty : télémétrie, signal de présence, compteur de joueurs, Discord Rich Presence et recherche de la cape « early player » sont désactivés.
- La fonction « Amis » dépend du réseau Misty et ne fonctionne donc plus.

## Compiler

```
cd misty-rust
cargo check --all-targets
cargo test
```

Windows uniquement (WebView2, registre, raccourcis).
