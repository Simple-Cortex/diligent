//! Embedded language subtag lists and a BCP 47 syntax check for `lang` values.
//!
//! Sources, authored by hand on 2026-10-18 with no network access:
//!
//! * ISO 639-1 two-letter codes (184 assigned codes), plus the five codes
//!   that IANA keeps as deprecated aliases and browsers still accept:
//!   `iw` (he), `in` (id), `ji` (yi), `jw` (jv) and `mo` (ro).
//! * ISO 639-2 three-letter codes, bibliographic and terminologic forms, and
//!   the reserved range `qaa` to `qtz`.
//! * BCP 47 (RFC 5646) section 2.1 for the tag syntax.
//!
//! BCP 47 says to use the shortest ISO 639 code, so a three-letter code that has
//! a two-letter form (`eng` for `en`) is [`LangCheck::Invalid`]. A three-letter
//! code without one (`fil`, `haw`) is valid. The `three:two` pairs, B and T
//! forms included, are in `THREE_TO_TWO`.
//!
//! A three-letter primary subtag outside the ISO 639-2 list may still be a
//! real ISO 639-3 language (`yue` is one), so [`check_lang`] reports it as
//! [`LangCheck::Unlisted`] rather than invalid. Only shapes that can never be
//! a language subtag, and two-letter codes that are not assigned, are
//! [`LangCheck::Invalid`].

/// ISO 639-1 codes and the IANA deprecated aliases.
const ISO_639_1: &str = "aa ab ae af ak am an ar as av ay az ba be bg bh bi bm bn bo br bs ca ce ch co cr cs cu cv cy da de dv dz ee el en eo es et eu fa ff fi fj fo fr fy ga gd gl gn gu gv ha he hi ho hr ht hu hy hz ia id ie ig ii ik io is it iu ja jv ka kg ki kj kk kl km kn ko kr ks ku kv kw ky la lb lg li ln lo lt lu lv mg mh mi mk ml mn mr ms mt my na nb nd ne ng nl nn no nr nv ny oc oj om or os pa pi pl ps pt qu rm rn ro ru rw sa sc sd se sg si sk sl sm sn so sq sr ss st su sv sw ta te tg th ti tk tl tn to tr ts tt tw ty ug uk ur uz ve vi vo wa wo xh yi yo za zh zu iw in ji jw mo";

/// ISO 639-2 codes, B and T forms, space separated.
const ISO_639_2: &str = "aar abk ace ach ada ady afa afh afr ain aka akk alb sqi ale alg alt amh ang anp apa ara arc arg arm hye arn arp art arw asm ast ath aus ava ave awa aym aze bad bai bak bal bam ban baq eus bas bat bej bel bem ben ber bho bih bik bin bis bla bnt bos bra bre btk bua bug bul bur mya byn cad cai car cat cau ceb cel cha chb che chg chi zho chk chm chn cho chp chr chu chv chy cmc cnr cop cor cos cpe cpf cpp cre crh crp csb cus cze ces dak dan dar day del den dgr din div doi dra dsb dua dum dut nld dyu dzo efi egy eka elx eng enm epo est ewe ewo fan fao fat fij fil fin fiu fon fre fra frm fro frr frs fry ful fur gaa gay gba gem geo kat ger deu gez gil gla gle glg glv gmh goh gon gor got grb grc gre ell grn gsw guj gwi hai hat hau haw heb her hil him hin hit hmn hmo hrv hsb hun hup iba ibo ice isl ido iii ijo iku ile ilo ina inc ind ine inh ipk ira iro ita jav jbo jpn jpr jrb kaa kab kac kal kam kan kar kas kau kaw kaz kbd kha khi khm kho kik kin kir kmb kok kom kon kor kos kpe krc krl kro kru kua kum kur kut lad lah lam lao lat lav lez lim lin lit lol loz ltz lua lub lug lui lun luo lus mac mkd mad mag mah mai mak mal man mao mri map mar mas may msa mdf mdr men mga mic min mis mkh mlg mlt mnc mni mno moh mon mos mul mun mus mwl mwr myn myv nah nai nap nau nav nbl nde ndo nds nep new nia nic niu nno nob nog non nor nqo nso nub nwc nya nym nyn nyo nzi oci oji ori orm osa oss ota oto paa pag pal pam pan pap pau peo per fas phi phn pli pol pon por pra pro pus que raj rap rar roa roh rom rum ron run rup rus sad sag sah sai sal sam san sas sat scn sco sel sem sga sgn shn sid sin sio sit sla slo slk slv sma sme smi smj smn smo sms sna snd snk sog som son sot spa srd srn srp srr ssa ssw suk sun sus sux swa swe syc syr tah tai tam tat tel tem ter tet tgk tgl tha tib bod tig tir tiv tkl tlh tli tmh tog ton tpi tsi tsn tso tuk tum tup tur tut tvl twi tyv udm uga uig ukr umb und urd uzb vai ven vie vol vot wak wal war was wel cym wen wln wol xal xho yao yap yid yor ypk zap zbl zen zgh zha znd zul zun zxx zza";

/// ISO 639-2 codes that have an ISO 639-1 equivalent, as `three:two` pairs.
/// BCP 47 says to use the shortest code, so the three-letter form is not accepted for these.
const THREE_TO_TWO: &str = "aar:aa abk:ab afr:af aka:ak amh:am ara:ar arg:an asm:as ava:av ave:ae aym:ay aze:az bak:ba bam:bm bel:be ben:bn bih:bh bis:bi bod:bo tib:bo bos:bs bre:br bul:bg cat:ca ces:cs cze:cs cha:ch che:ce chu:cu chv:cv cor:kw cos:co cre:cr cym:cy wel:cy dan:da deu:de ger:de div:dv dzo:dz ell:el gre:el eng:en epo:eo est:et eus:eu baq:eu ewe:ee fao:fo fas:fa per:fa fij:fj fin:fi fra:fr fre:fr fry:fy ful:ff gla:gd gle:ga glg:gl glv:gv grn:gn guj:gu hat:ht hau:ha heb:he her:hz hin:hi hmo:ho hrv:hr hun:hu hye:hy arm:hy ibo:ig ido:io iii:ii iku:iu ile:ie ina:ia ind:id ipk:ik isl:is ice:is ita:it jav:jv jpn:ja kal:kl kan:kn kas:ks kat:ka geo:ka kau:kr kaz:kk khm:km kik:ki kin:rw kir:ky kom:kv kon:kg kor:ko kua:kj kur:ku lao:lo lat:la lav:lv lim:li lin:ln lit:lt ltz:lb lub:lu lug:lg mah:mh mal:ml mar:mr mkd:mk mac:mk mlg:mg mlt:mt mon:mn mri:mi mao:mi msa:ms may:ms mya:my bur:my nau:na nav:nv nbl:nr nde:nd ndo:ng nep:ne nld:nl dut:nl nno:nn nob:nb nor:no nya:ny oci:oc oji:oj ori:or orm:om oss:os pan:pa pli:pi pol:pl por:pt pus:ps que:qu roh:rm ron:ro rum:ro run:rn rus:ru sag:sg san:sa sin:si slk:sk slo:sk slv:sl sme:se smo:sm sna:sn snd:sd som:so sot:st spa:es sqi:sq alb:sq srd:sc srp:sr ssw:ss sun:su swa:sw swe:sv tah:ty tam:ta tat:tt tel:te tgk:tg tgl:tl tha:th tir:ti ton:to tsn:tn tso:ts tuk:tk tur:tr twi:tw uig:ug ukr:uk urd:ur uzb:uz ven:ve vie:vi vol:vo wln:wa wol:wo xho:xh yid:yi yor:yo zha:za zho:zh chi:zh zul:zu";

/// The ISO 639-1 code to use instead of the three-letter `code`, when there is one.
pub fn shorter_form(code: &str) -> Option<&'static str> {
    let c = code.to_ascii_lowercase();
    THREE_TO_TWO.split(' ').find_map(|pair| {
        pair.split_once(':')
            .filter(|(three, _)| *three == c)
            .map(|(_, two)| two)
    })
}

/// Outcome of checking a `lang` attribute value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LangCheck {
    /// Well-formed, and the primary language subtag is listed.
    Valid,
    /// Well-formed, but the three-letter primary subtag is not in the embedded list.
    Unlisted(String),
    /// Not a language tag; the string says why.
    Invalid(String),
}

fn listed(list: &str, code: &str) -> bool {
    list.split(' ').any(|c| c == code)
}

/// True when `code` is a listed ISO 639-1 or ISO 639-2 language code, or in the reserved `qaa`..`qtz` range.
pub fn is_known_language(code: &str) -> bool {
    let c = code.to_ascii_lowercase();
    match c.len() {
        2 => listed(ISO_639_1, &c),
        3 => listed(ISO_639_2, &c) || (c.as_str() >= "qaa" && c.as_str() <= "qtz"),
        _ => false,
    }
}

/// The primary language subtag of `value`, lowercase, when it has one.
pub fn primary_subtag(value: &str) -> Option<String> {
    let v = value.trim();
    let first = v.split('-').next()?;
    (!first.is_empty()).then(|| first.to_ascii_lowercase())
}

/// Checks a `lang` or `xml:lang` value.
///
/// Leading and trailing ASCII whitespace is ignored. The check covers the
/// primary subtag and the overall subtag shape (1 to 8 letters or digits,
/// separated by single hyphens). Later subtags are not looked up in the IANA
/// registry. Tags that start with `x-` (private use) are valid.
pub fn check_lang(value: &str) -> LangCheck {
    let v = value.trim_matches(|c: char| c.is_ascii_whitespace());
    if v.is_empty() {
        return LangCheck::Invalid("empty value".to_string());
    }
    let lower = v.to_ascii_lowercase();
    let subtags: Vec<&str> = lower.split('-').collect();
    if subtags
        .iter()
        .any(|s| s.is_empty() || s.len() > 8 || !s.bytes().all(|b| b.is_ascii_alphanumeric()))
    {
        return LangCheck::Invalid(format!("`{v}` is not a well-formed BCP 47 tag"));
    }
    let primary = subtags[0];
    if primary == "x" {
        return if subtags.len() > 1 {
            LangCheck::Valid
        } else {
            LangCheck::Invalid("private-use tag has no subtags".to_string())
        };
    }
    if primary == "i" && subtags.len() > 1 {
        // Grandfathered IANA tags (i-lux, i-klingon, …): the primary subtag `i`
        // is not a valid ISO 639 language code. BCP 47 says to use the modern
        // replacement (e.g. `lb` for `i-lux`), so these are invalid.
        return LangCheck::Invalid(format!(
            "`{v}` is a grandfathered tag; use its modern replacement instead"
        ));
    }
    if !primary.bytes().all(|b| b.is_ascii_alphabetic()) || !(2..=3).contains(&primary.len()) {
        return LangCheck::Invalid(format!("`{primary}` is not a language subtag"));
    }
    // Only one region subtag (two letters or three digits) may appear, before any singleton.
    let mut regions = 0;
    for sub in &subtags[1..] {
        if sub.len() == 1 {
            break;
        }
        let two_alpha = sub.len() == 2 && sub.bytes().all(|b| b.is_ascii_alphabetic());
        let three_digit = sub.len() == 3 && sub.bytes().all(|b| b.is_ascii_digit());
        if two_alpha || three_digit {
            regions += 1;
        }
    }
    if regions > 1 {
        return LangCheck::Invalid(format!("`{v}` repeats the region subtag"));
    }
    if let Some(two) = shorter_form(primary) {
        LangCheck::Invalid(format!(
            "`{primary}` has the shorter ISO 639-1 form `{two}`; BCP 47 says to use the shortest code"
        ))
    } else if is_known_language(primary) {
        LangCheck::Valid
    } else if primary.len() == 3 {
        LangCheck::Unlisted(primary.to_string())
    } else {
        LangCheck::Invalid(format!("`{primary}` is not an assigned ISO 639-1 code"))
    }
}
