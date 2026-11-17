//! Misty Client : le mod Fabric de Misty (repo Misty-MC/Misty-Client), qui apporte en jeu le
//! menu principal Mistly, le HUD, le zoom, la luminosité maximale, le nuage Mistly devant les
//! joueurs Mistly (liste des joueurs et chat) et les cosmétiques Mistly.
//!
//! Le mod n'est pas dans l'exe : il est téléchargé depuis les releases GitHub de Misty-Client.
//! Chaque release publie un jar par version de Minecraft et `misty-client.json`, la liste de ces
//! jars avec les versions de Minecraft où chacun tourne (voir son workflow `release.yml`). Au
//! démarrage, `actualiser` lit la dernière release et garde cette liste dans
//! `%LOCALAPPDATA%\Misty\misty-client\index.json` ; le jar d'une version n'est téléchargé qu'au
//! premier lancement qui en a besoin (`telecharger`), puis gardé à côté. Sans Internet, la liste
//! et les jars déjà téléchargés servent encore.
//!
//! Développement : `MISTY_CLIENT_DIR=<dossier du mod>` utilise les jars compilés de ce dossier
//! (`versions/<minecraft>/build/libs/`) au lieu de GitHub (c'est ce que fait « Lancer Misty.cmd »).
//!
//! Deux façons de jouer avec : la section « Mistly Client » du choix de version (Fabric + le mod,
//! sur une des versions où il existe), et le réglage « Mistly Client en jeu », qui le pose dans le
//! dossier `mods` des profils Fabric de ces versions. Il est retiré partout ailleurs. Le jeu reçoit
//! le dossier de Misty (`-Dmisty.launcher.dir`) : le mod y lit `client.json`, où le launcher
//! écrit le cosmétique équipé.

use super::instances::Instance;
use super::{download, http, journal, paths, settings};
use crate::minecraft::loaders;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Repo GitHub du mod (ses releases portent les jars).
pub const DEPOT: &str = "Misty-MC/Misty-Client";
/// Liste des jars publiée avec chaque release.
const MANIFESTE: &str = "misty-client.json";

/// Nom du mod dans le dossier `mods` : toujours le même, pour remplacer l'ancien à chaque mise à jour.
const FICHIER: &str = "misty-client.jar";

/// Cosmétiques Misty, tels que le mod et le site les nomment. Dans le dos : une cape ou des élytres.
pub const CAPE: &str = "misty_cape";
pub const ELYTRES: &str = "misty_elytra";
/// Cape des premiers joueurs (le site dit qui y a droit : `early()`).
pub const CAPE_EARLY: &str = "early_cape";
/// Sur la tête : un petit nuage.
pub const CLOUDLY: &str = "cloudly";

/// Styles du pseudo au-dessus de la tête (le tag) ; "" : le style classique.
pub const TAGS: &[&str] = &["nuage"];

const DOS: &[&str] = &[CAPE, ELYTRES, CAPE_EARLY];
const CHAPEAUX: &[&str] = &[CLOUDLY];

// ───────────────────────── Les jars : liste, téléchargement ─────────────────────────

/// Un jar du mod.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Jar {
    /// nom du fichier : "misty-client-1.0.2+mc26.1.2.jar"
    pub fichier: String,
    /// versions de Minecraft où il tourne
    pub minecraft: Vec<String>,
    /// adresse de téléchargement ("" : jar local, en développement)
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    /// jar local (développement) : son chemin
    #[serde(skip)]
    pub local: Option<PathBuf>,
}

/// Les jars connus : ceux de la dernière release lue (ou du dossier de développement).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Index {
    /// tag de la release
    #[serde(default)]
    release: String,
    jars: Vec<Jar>,
}

static INDEX: RwLock<Option<Index>> = RwLock::new(None);

/// Dossier des jars téléchargés et de la liste.
fn dossier() -> PathBuf {
    paths::data_dir().join("misty-client")
}

/// Dossier du mod en développement (`MISTY_CLIENT_DIR`), s'il est donné.
fn dossier_dev() -> Option<PathBuf> {
    std::env::var("MISTY_CLIENT_DIR").ok().filter(|d| !d.trim().is_empty()).map(PathBuf::from)
}

fn index() -> Index {
    if let Some(i) = INDEX.read().unwrap().as_ref() {
        return i.clone();
    }
    let i = match dossier_dev() {
        Some(d) => index_local(&d),
        None => fs::read_to_string(dossier().join("index.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default(),
    };
    *INDEX.write().unwrap() = Some(i.clone());
    i
}

/// Jars compilés d'un dossier du mod : `versions/<minecraft>/build/libs/misty-client-<v>+mc<mc>.jar`
/// (le plus récent par version), et les versions de Minecraft de chacun (`mod.mc_releases` de
/// `stonecutter.properties.toml`, sinon celle de son nom).
fn index_local(racine: &Path) -> Index {
    let releases = mc_releases(&fs::read_to_string(racine.join("stonecutter.properties.toml")).unwrap_or_default());
    let mut jars: std::collections::BTreeMap<String, (std::time::SystemTime, PathBuf)> = Default::default();
    for v in fs::read_dir(racine.join("versions")).into_iter().flatten().flatten() {
        for f in fs::read_dir(v.path().join("build/libs")).into_iter().flatten().flatten() {
            let nom = f.file_name().to_string_lossy().to_string();
            let Some(mc) = mc_du_nom(&nom) else { continue };
            let Ok(date) = f.metadata().and_then(|m| m.modified()) else { continue };
            if jars.get(&mc).is_none_or(|(d, _)| date > *d) {
                jars.insert(mc, (date, f.path()));
            }
        }
    }
    let jars = jars
        .into_iter()
        .map(|(mc, (_, chemin))| Jar {
            fichier: chemin.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            minecraft: releases.get(&mc).cloned().unwrap_or_else(|| vec![mc.clone()]),
            url: String::new(),
            sha1: None,
            local: Some(chemin),
        })
        .collect();
    Index { release: "dev".into(), jars }
}

/// "misty-client-1.0.2+mc26.1.2.jar" → "26.1.2" (pas les jars de sources)
fn mc_du_nom(nom: &str) -> Option<String> {
    if nom.ends_with("-sources.jar") {
        return None;
    }
    nom.strip_prefix("misty-client-")?.strip_suffix(".jar")?.split_once("+mc").map(|(_, mc)| mc.to_string())
}

/// `["26.1.2"]` puis `mod.mc_releases = ["26.1", "26.1.2"]` : version du jar → versions de Minecraft.
fn mc_releases(toml: &str) -> std::collections::HashMap<String, Vec<String>> {
    let mut liste = std::collections::HashMap::new();
    let mut section = None;
    for ligne in toml.lines().map(str::trim) {
        if let Some(nom) = ligne.strip_prefix("[\"").and_then(|l| l.strip_suffix("\"]")) {
            section = Some(nom.to_string());
        } else if let (Some(nom), Some(valeur)) = (&section, ligne.strip_prefix("mod.mc_releases")) {
            let versions = valeur.split('"').skip(1).step_by(2).map(str::to_string).collect();
            liste.insert(nom.clone(), versions);
        }
    }
    liste
}

/// "1.21.11" < "26.1" < "26.1.2" < "26.3" (nombre par nombre)
fn ordre_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let n = |v: &str| v.split(['.', '-']).map(|x| x.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    n(a).cmp(&n(b))
}

/// Au démarrage : lit la dernière release du mod sur GitHub et garde sa liste de jars. Renvoie
/// vrai si la liste a changé (l'interface doit alors se mettre à jour). En développement
/// (`MISTY_CLIENT_DIR`), rien à lire.
pub async fn actualiser() -> anyhow::Result<bool> {
    if dossier_dev().is_some() {
        return Ok(false);
    }
    *DERNIERE_LECTURE.lock().unwrap() = Some(std::time::Instant::now());
    // la dernière release publiée (une pré-release seulement si on suit les pré-releases)
    let pre = settings::get_bool(settings::MAJ_PRERELEASE, false);
    let liste: Vec<serde_json::Value> = http::get_json(&format!("https://api.github.com/repos/{DEPOT}/releases?per_page=20")).await?;
    let release = liste
        .iter()
        .filter(|r| !r["draft"].as_bool().unwrap_or(false))
        .find(|r| pre || !r["prerelease"].as_bool().unwrap_or(false))
        .ok_or_else(|| anyhow::anyhow!("{DEPOT} : aucune release publiée"))?;
    let tag = release["tag_name"].as_str().unwrap_or_default().to_string();
    let assets = release["assets"].as_array().cloned().unwrap_or_default();
    let url_de = |nom: &str| assets.iter().find(|a| a["name"].as_str() == Some(nom)).and_then(|a| a["browser_download_url"].as_str()).map(str::to_string);
    let manifeste = url_de(MANIFESTE).ok_or_else(|| anyhow::anyhow!("{DEPOT} {tag} : pas de {MANIFESTE}"))?;
    let doc: serde_json::Value = http::get_json(&manifeste).await?;
    let mut jars = Vec::new();
    for j in doc["jars"].as_array().into_iter().flatten() {
        let fichier = j["file"].as_str().unwrap_or_default().to_string();
        let minecraft: Vec<String> = j["minecraft"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect();
        // seulement les jars vraiment publiés avec la release
        let Some(url) = url_de(&fichier) else { continue };
        if minecraft.is_empty() || mc_du_nom(&fichier).is_none() {
            continue;
        }
        jars.push(Jar { fichier, minecraft, url, sha1: j["sha1"].as_str().map(str::to_string), local: None });
    }
    anyhow::ensure!(!jars.is_empty(), "{DEPOT} {tag} : aucun jar dans {MANIFESTE}");
    let nouveau = Index { release: tag, jars };
    let texte = serde_json::to_string_pretty(&nouveau)?;
    let chemin = dossier().join("index.json");
    let change = fs::read_to_string(&chemin).ok().as_deref() != Some(texte.as_str());
    if change {
        fs::create_dir_all(dossier())?;
        fs::write(&chemin, &texte)?;
        journal::info(&format!("Mistly Client {} : {}", nouveau.release, nouveau.jars.iter().map(|j| j.fichier.as_str()).collect::<Vec<_>>().join(", ")));
        // les jars des anciennes releases ne servent plus
        for f in fs::read_dir(dossier()).into_iter().flatten().flatten() {
            let nom = f.file_name().to_string_lossy().to_string();
            if nom.ends_with(".jar") && !nouveau.jars.iter().any(|j| j.fichier == nom) {
                let _ = fs::remove_file(f.path());
            }
        }
    }
    *INDEX.write().unwrap() = Some(nouveau);
    Ok(change)
}

/// Quand la release a été lue pour la dernière fois.
static DERNIERE_LECTURE: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Avant une partie : relit la release si la dernière lecture date de plus de 30 minutes (le
/// launcher peut rester ouvert des jours, une nouvelle version du mod doit quand même arriver).
/// Renvoie vrai si la liste des jars a changé.
pub async fn actualiser_si_ancien() -> bool {
    let ancienne = DERNIERE_LECTURE.lock().unwrap().is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(30 * 60));
    if !ancienne {
        return false;
    }
    match tokio::time::timeout(std::time::Duration::from_secs(10), actualiser()).await {
        Ok(Ok(change)) => change,
        Ok(Err(e)) => {
            journal::info(&format!("Mistly Client : release non relue ({e:#})"));
            false
        }
        Err(_) => false,
    }
}

/// Le jar du mod pour cette version de Minecraft.
fn jar_pour(version_mc: &str) -> Option<Jar> {
    index().jars.into_iter().find(|j| j.minecraft.iter().any(|m| m == version_mc))
}

/// Chemin du jar sur le disque (téléchargé ou non).
fn chemin(jar: &Jar) -> PathBuf {
    jar.local.clone().unwrap_or_else(|| dossier().join(&jar.fichier))
}

/// Le jar de cette version, téléchargé s'il ne l'est pas encore. Renvoie son chemin.
pub async fn telecharger(version_mc: &str) -> anyhow::Result<PathBuf> {
    let jar = jar_pour(version_mc).ok_or_else(|| anyhow::anyhow!("Mistly Client n'existe pas pour Minecraft {version_mc}"))?;
    let chemin = chemin(&jar);
    if jar.local.is_some() {
        return Ok(chemin);
    }
    // déjà téléchargé : gardé s'il est intact (même SHA-1 que celui de la release)
    if chemin.is_file() {
        let attendu = jar.sha1.clone();
        let c = chemin.clone();
        let intact = tokio::task::spawn_blocking(move || match attendu {
            Some(sha1) => download::sha1_fichier(&c).is_ok_and(|h| h.eq_ignore_ascii_case(&sha1)),
            None => true,
        })
        .await
        .unwrap_or(false);
        if intact {
            return Ok(chemin);
        }
        journal::info(&format!("Mistly Client : {} ne correspond plus à la release, nouveau téléchargement", jar.fichier));
    }
    journal::info(&format!("Téléchargement de Mistly Client : {}", jar.fichier));
    download::fichier(&jar.url, &chemin, jar.sha1.as_deref()).await?;
    Ok(chemin)
}

/// Le jar de cette version s'il est déjà sur le disque.
fn jar_sur_disque(version_mc: &str) -> Option<Vec<u8>> {
    fs::read(chemin(&jar_pour(version_mc)?)).ok()
}

/// Le mod existe-t-il (pour au moins une version) ?
pub fn disponible() -> bool {
    !index().jars.is_empty()
}

/// Les versions de Minecraft où Misty Client existe, de la plus ancienne à la plus récente.
pub fn versions() -> Vec<String> {
    let mut v: Vec<String> = index().jars.into_iter().flat_map(|j| j.minecraft).collect();
    v.sort_by(|a, b| ordre_versions(a, b));
    v.dedup();
    v
}

/// La version de Minecraft la plus récente où Misty Client existe ("" sans mod).
pub fn derniere() -> String {
    versions().pop().unwrap_or_default()
}

/// Misty Client existe-t-il pour cette version de Minecraft ?
pub fn existe(version_mc: &str) -> bool {
    jar_pour(version_mc).is_some()
}

/// Les versions du mod en une ligne, pour l'affichage ("1.21.11, 26.1, 26.3").
pub fn versions_texte() -> String {
    versions().join(", ")
}

/// Case « Misty Client » cochée de base dans les profils Fabric (Options → Dépannage, activé de
/// base). Vaut aussi pour le jeu rapide en Fabric.
pub fn active() -> bool {
    settings::get_bool(settings::MISTY_CLIENT, true)
}

/// Misty Client voulu dans ce profil : sa case, ou le réglage par défaut s'il n'y a pas touché.
pub fn voulu(instance: &Instance) -> bool {
    instance.misty_client.unwrap_or_else(active)
}

/// Le mod tourne-t-il sur ce profil ? (Fabric, sur une version de Minecraft où il existe)
pub fn compatible(version_mc: &str, mode: &str) -> bool {
    mode == loaders::FABRIC && existe(version_mc)
}

/// Version d'un jar de Misty Client, lue dans son fabric.mod.json ("1.0.0+mc26.2"). None : ce
/// n'est pas Misty Client.
fn version_du_jar(contenu: impl std::io::Read + std::io::Seek) -> Option<String> {
    let mut zip = zip::ZipArchive::new(contenu).ok()?;
    let mut fichier = zip.by_name("fabric.mod.json").ok()?;
    let mut texte = String::new();
    std::io::Read::read_to_string(&mut fichier, &mut texte).ok()?;
    let json: serde_json::Value = serde_json::from_str(&texte).ok()?;
    (json["id"].as_str() == Some("misty-client")).then(|| json["version"].as_str().unwrap_or_default().to_string())
}

/// "1.0.0+mc26.2" → ("1.0.0", "26.2")
fn decouper(version: &str) -> (&str, &str) {
    version.split_once("+mc").unwrap_or((version, ""))
}

/// Le jar du mod pour cette version et son nom de fichier avec la version
/// ("misty-client-1.0.0+mc26.2.jar"), pour l'export d'un profil. Il doit déjà être téléchargé
/// (`telecharger`).
pub fn jar_a_exporter(version_mc: &str) -> Option<(String, Vec<u8>)> {
    let jar = jar_sur_disque(version_mc)?;
    let version = version_du_jar(std::io::Cursor::new(&jar)).unwrap_or_default();
    Some((format!("misty-client-{version}.jar"), jar))
}

/// Les jars de Misty Client d'un dossier `mods`, avec leur version : quel que soit leur nom (un
/// modpack exporté les nomme avec leur version, d'anciens launchers « misty-client.jar »).
pub fn jars_presents(mods: &Path) -> Vec<(std::path::PathBuf, String)> {
    let Ok(lecture) = fs::read_dir(mods) else { return Vec::new() };
    lecture
        .flatten()
        .map(|f| f.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jar")))
        .filter_map(|p| {
            let version = version_du_jar(fs::File::open(&p).ok()?)?;
            Some((p, version))
        })
        .collect()
}

/// Avant de lancer le jeu : pose le mod dans `mods` (ou le met à jour) si `voulu` et que le profil
/// le supporte, sinon le retire (un mod d'une autre version de Minecraft empêcherait le jeu de
/// démarrer). `notre` : le jar de cette version (téléchargé avant, voir `telecharger`) ; sans lui,
/// le jar déjà dans `mods` reste s'il est pour cette version. Les jars du mod déjà là sont
/// comparés au nôtre : les anciens (ou ceux d'une autre version de Minecraft) sont supprimés, pour
/// qu'il n'en reste jamais qu'un ; un plus récent que le nôtre (modpack exporté plus tard) est gardé.
pub fn preparer(dossier_jeu: &Path, version_mc: &str, mode: &str, voulu: bool, notre: Option<&[u8]>) {
    let mods = dossier_jeu.join("mods");
    let presents = jars_presents(&mods);
    if !(voulu && compatible(version_mc, mode)) {
        for (chemin, _) in presents {
            let _ = fs::remove_file(&chemin);
        }
        return;
    }
    let Some(notre) = notre else {
        // pas de jar (téléchargement impossible) : on garde celui de cette version s'il y en a un
        for (chemin, v) in &presents {
            if !decouper(v).1.is_empty() && !jar_pour(version_mc).is_some_and(|j| mc_du_nom(&j.fichier).as_deref() == Some(decouper(v).1)) {
                let _ = fs::remove_file(chemin);
            }
        }
        return;
    };
    let version = version_du_jar(std::io::Cursor::new(notre)).unwrap_or_default();
    let (numero, mc) = decouper(&version);

    // le plus récent des jars déjà là, s'il est pour cette version de Minecraft et plus récent que le nôtre
    let garde = presents
        .iter()
        .filter(|(_, v)| decouper(v).1 == mc && super::update::comparer_versions(decouper(v).0, numero).is_gt())
        .max_by(|a, b| super::update::comparer_versions(decouper(&a.1).0, decouper(&b.1).0))
        .map(|(p, _)| p.clone());
    let chemin = garde.clone().unwrap_or_else(|| mods.join(FICHIER));
    for (autre, v) in &presents {
        if *autre != chemin {
            journal::info(&format!("Ancien Misty Client retiré : {} ({v})", autre.display()));
            let _ = fs::remove_file(autre);
        }
    }
    if garde.is_some() || fs::read(&chemin).is_ok_and(|actuel| actuel == notre) {
        return;
    }
    let ecrit = fs::create_dir_all(&mods).and_then(|_| fs::write(&chemin, notre));
    if let Err(e) = ecrit {
        journal::info(&format!("Misty Client non installé dans {} : {e}", chemin.display()));
    }
}

/// La garde-robe du jeu (Misty Client) écrit son choix dans `client.json` avec `"from": "game"` :
/// le launcher le reprend au lieu d'écrire le sien par-dessus.
pub fn reprendre_du_jeu() {
    let Ok(texte) = fs::read_to_string(paths::data_dir().join("client.json")) else { return };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&texte) else { return };
    if json["from"].as_str() != Some("game") {
        return;
    }
    let choix = json["cosmetic"].as_str().unwrap_or("");
    if choix.is_empty() || DOS.contains(&choix) {
        if choix != settings::get(settings::COSMETIQUE) {
            settings::set(settings::COSMETIQUE, choix);
            journal::info(&format!("Cosmétique choisi en jeu : {}", if choix.is_empty() { "aucun" } else { choix }));
        }
    }
    let tag = json["tag"].as_str().unwrap_or("");
    if (tag.is_empty() || TAGS.contains(&tag)) && tag != settings::get(settings::TAG) {
        settings::set(settings::TAG, tag);
    }
    let chapeau = json["hat"].as_str().unwrap_or("");
    if (chapeau.is_empty() || CHAPEAUX.contains(&chapeau)) && chapeau != settings::get(settings::CHAPEAU) {
        settings::set(settings::CHAPEAU, chapeau);
        journal::info(&format!("Chapeau choisi en jeu : {}", if chapeau.is_empty() { "aucun" } else { chapeau }));
    }
}

/// Cosmétique Misty porté dans le dos ("" : aucun).
pub fn cosmetique() -> String {
    reprendre_du_jeu();
    let c = settings::get(settings::COSMETIQUE);
    if DOS.contains(&c.as_str()) {
        c
    } else {
        String::new()
    }
}

/// Chapeau Misty porté ("" : aucun).
pub fn chapeau() -> String {
    reprendre_du_jeu();
    let c = settings::get(settings::CHAPEAU);
    if CHAPEAUX.contains(&c.as_str()) {
        c
    } else {
        String::new()
    }
}

/// Style du pseudo au-dessus de la tête ("" : classique).
pub fn tag() -> String {
    reprendre_du_jeu();
    let t = settings::get(settings::TAG);
    if TAGS.contains(&t.as_str()) {
        t
    } else {
        String::new()
    }
}

/// Change le style du pseudo (le mod le voit tout de suite, même en pleine partie).
pub fn equiper_tag(tag: &str) {
    let tag = if TAGS.contains(&tag) { tag } else { "" };
    if tag != settings::get(settings::TAG) {
        settings::set(settings::TAG, tag);
    }
    ecrire_lien();
}

/// Met (ou enlève : "") le chapeau Misty.
pub fn equiper_chapeau(chapeau: &str) {
    if chapeau != settings::get(settings::CHAPEAU) {
        settings::set(settings::CHAPEAU, chapeau);
    }
    ecrire_lien();
}

/// Ce compte a-t-il droit à la cape early player ? (le site le sait : comptes vus avant une date)
pub async fn early(uuid: &str) -> bool {
    let url = format!("https://projet.invalid/api/client/early/{uuid}");
    if true {
        return false;
    }
    super::http::get_json::<serde_json::Value>(&url).await.ok().and_then(|v| v["early"].as_bool()).unwrap_or(false)
}

/// Équipe un cosmétique Misty (ou aucun) : il est gardé d'une session à l'autre et le mod le
/// voit tout de suite, même en pleine partie.
pub fn equiper(cosmetique: &str) {
    if cosmetique != settings::get(settings::COSMETIQUE) {
        settings::set(settings::COSMETIQUE, cosmetique);
    }
    ecrire_lien();
}

/// Écrit `client.json` (lu par le mod) dans le dossier de Misty.
pub fn ecrire_lien() {
    let contenu = serde_json::json!({ "cosmetic": cosmetique(), "hat": chapeau(), "tag": tag() });
    let chemin = paths::data_dir().join("client.json");
    if fs::read_to_string(&chemin).is_ok_and(|actuel| actuel == contenu.to_string()) {
        return;
    }
    let _ = fs::write(chemin, contenu.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un faux jar de Misty Client (juste son fabric.mod.json).
    fn faux_jar(version: &str) -> Vec<u8> {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default()).unwrap();
        write!(zip, r#"{{"id": "misty-client", "version": "{version}"}}"#).unwrap();
        zip.finish().unwrap().into_inner()
    }

    /// Une liste de jars comme celle d'une release : 26.1.2 tourne aussi sur 26.1 et 26.1.1.
    fn index_test() {
        let jar = |fichier: &str, mc: &[&str]| Jar { fichier: fichier.into(), minecraft: mc.iter().map(|m| m.to_string()).collect(), url: String::new(), sha1: None, local: None };
        *INDEX.write().unwrap() = Some(Index {
            release: "test".into(),
            jars: vec![
                jar("misty-client-1.0.0+mc26.3.jar", &["26.3"]),
                jar("misty-client-1.0.0+mc26.1.2.jar", &["26.1", "26.1.1", "26.1.2"]),
                jar("misty-client-1.0.0+mc1.21.11.jar", &["1.21.11"]),
            ],
        });
    }

    #[test]
    fn profils_compatibles() {
        index_test();
        assert_eq!(versions(), ["1.21.11", "26.1", "26.1.1", "26.1.2", "26.3"]);
        assert_eq!(derniere(), "26.3");
        for mc in versions() {
            assert!(compatible(&mc, loaders::FABRIC));
            assert!(!compatible(&mc, loaders::VANILLA));
            assert!(!compatible(&mc, loaders::FORGE));
        }
        assert!(!compatible("0.0.1", loaders::FABRIC));
        assert_eq!(jar_pour("26.1.1").unwrap().fichier, "misty-client-1.0.0+mc26.1.2.jar");
    }

    #[test]
    fn noms_et_versions_du_mod() {
        assert_eq!(mc_du_nom("misty-client-1.0.2+mc26.1.2.jar").as_deref(), Some("26.1.2"));
        assert_eq!(mc_du_nom("misty-client-1.0.2+mc26.1.2-sources.jar"), None);
        assert_eq!(mc_du_nom("autre.jar"), None);
        let toml = "mod.version = \"1\"\n[\"26.1.2\"]\nmod.mc_releases = [\"26.1\", \"26.1.2\"]\n[\"26.3\"]\nmod.mc_releases = [\"26.3\"]\n";
        let r = mc_releases(toml);
        assert_eq!(r["26.1.2"], ["26.1", "26.1.2"]);
        assert_eq!(r["26.3"], ["26.3"]);
    }

    /// Développement : les jars compilés du clone Misty-Client à côté du launcher (s'il est compilé).
    #[test]
    fn jars_locaux() {
        let racine = Path::new("../../Misty-Client");
        if !racine.join("versions").is_dir() {
            return;
        }
        let index = index_local(racine);
        if index.jars.is_empty() {
            return;
        }
        let j = index.jars.iter().find(|j| j.fichier.contains("+mc26.1.2")).expect("jar 26.1.2");
        assert!(j.minecraft.iter().any(|m| m == "26.1"), "{:?}", j.minecraft);
        assert!(j.local.as_ref().is_some_and(|p| p.is_file()));
    }

    /// Les anciens jars du mod (autre nom, autre version de Minecraft) sont retirés, un seul reste.
    #[test]
    fn un_seul_jar_du_mod() {
        index_test();
        let mc = "26.3";
        let notre = faux_jar("1.0.0+mc26.3");
        let dossier = std::env::temp_dir().join(format!("misty-test-mods-{}", std::process::id()));
        let mods = dossier.join("mods");
        let _ = fs::remove_dir_all(&dossier);
        fs::create_dir_all(&mods).unwrap();
        fs::write(mods.join("misty-client-0.0.1+mc26.3.jar"), faux_jar("0.0.1+mc26.3")).unwrap();
        fs::write(mods.join("ancien.jar"), faux_jar("9.9.9+mc0.0")).unwrap();
        fs::write(mods.join("autre-mod.jar"), b"pas un zip").unwrap();
        preparer(&dossier, mc, loaders::FABRIC, true, Some(&notre));
        let restants = jars_presents(&mods);
        assert_eq!(restants.len(), 1, "{restants:?}");
        assert_eq!(restants[0].0, mods.join(FICHIER));
        assert!(mods.join("autre-mod.jar").exists());
        // téléchargement impossible : celui de cette version reste, celui d'une autre version part
        fs::write(mods.join("vieux.jar"), faux_jar("0.0.1+mc26.1.2")).unwrap();
        preparer(&dossier, mc, loaders::FABRIC, true, None);
        let restants = jars_presents(&mods);
        assert_eq!(restants.len(), 1, "{restants:?}");
        assert_eq!(restants[0].0, mods.join(FICHIER));
        // décoché : plus aucun
        preparer(&dossier, mc, loaders::FABRIC, false, Some(&notre));
        assert!(jars_presents(&mods).is_empty());
        // un plus récent que le nôtre est gardé
        fs::write(mods.join("recent.jar"), faux_jar("99.0.0+mc26.3")).unwrap();
        preparer(&dossier, mc, loaders::FABRIC, true, Some(&notre));
        let restants = jars_presents(&mods);
        assert_eq!(restants.len(), 1);
        assert_eq!(restants[0].0, mods.join("recent.jar"));
        let _ = fs::remove_dir_all(&dossier);
    }

    /// La dernière release de Misty-Client sur GitHub a bien sa liste de jars (Internet).
    #[tokio::test]
    #[ignore]
    async fn release_github() {
        if dossier_dev().is_some() {
            return;
        }
        actualiser().await.unwrap();
        assert!(disponible());
        let mc = derniere();
        let chemin = telecharger(&mc).await.unwrap();
        assert!(version_du_jar(fs::File::open(chemin).unwrap()).is_some());
    }

    /// La cape du mod est celle que le launcher dessine dans l'onglet Skin : mêmes pixels.
    #[test]
    fn cape_du_mod_identique() {
        let mod_ = image::load_from_memory(include_bytes!("../../../misty-client/src/main/resources/assets/misty/textures/cape/misty.png")).unwrap().to_rgba8();
        let launcher = image::load_from_memory(crate::services::skins::texture_misty()).unwrap().to_rgba8();
        assert!(mod_ == launcher, "misty-client : textures/cape/misty.png n'est plus la texture de skins::texture_misty()");
    }
}
