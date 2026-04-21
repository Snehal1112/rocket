use fake::Fake;
use rand::Rng;

/// Check if a name (without the `$` prefix) is a known dynamic variable.
pub fn is_dynamic_var(name: &str) -> bool {
    generate(name).is_some()
}

/// Generate a fresh value for a dynamic variable (name without `$` prefix).
/// Returns `None` if the name is not a known dynamic variable.
pub fn generate(name: &str) -> Option<String> {
    let mut rng = rand::thread_rng();
    match name {
        // ── Basic Data Types ──
        "guid" | "randomUUID" => Some(uuid::Uuid::new_v4().to_string()),
        "timestamp" => Some(chrono::Utc::now().timestamp().to_string()),
        "isoTimestamp" => Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        "randomNanoId" => {
            let charset: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
            let id: String = (0..21).map(|_| charset[rng.gen_range(0..charset.len())] as char).collect();
            Some(id)
        }
        "randomAlphaNumeric" => {
            let charset: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
            let c = charset[rng.gen_range(0..charset.len())] as char;
            Some(c.to_string())
        }
        "randomBoolean" => Some(rng.gen_bool(0.5).to_string()),
        "randomInt" => Some(rng.gen_range(0..=1000).to_string()),
        "randomColor" => {
            let colors = ["red", "green", "blue", "yellow", "purple", "cyan", "magenta",
                          "white", "black", "orange", "pink", "grey", "fuchsia", "lime",
                          "maroon", "navy", "olive", "teal", "violet", "turquoise"];
            Some(colors[rng.gen_range(0..colors.len())].to_string())
        }
        "randomHexColor" => Some(format!("#{:06x}", rng.gen::<u32>() & 0xFFFFFF)),
        "randomAbbreviation" => {
            let abbrs = ["SQL", "PCI", "JSON", "HTTP", "SSL", "TCP", "UDP", "API", "XML",
                         "CSS", "HTML", "FTP", "SSH", "DNS", "RAM", "CPU", "GPU", "USB",
                         "URL", "CLI", "GUI", "SDK", "IDE", "REST", "SMTP"];
            Some(abbrs[rng.gen_range(0..abbrs.len())].to_string())
        }
        "randomWord" => {
            let word: String = fake::faker::lorem::en::Word().fake_with_rng(&mut rng);
            Some(word)
        }
        "randomWords" => {
            let words: Vec<String> = fake::faker::lorem::en::Words(3..5).fake_with_rng(&mut rng);
            Some(words.join(" "))
        }

        // ── Internet and Network ──
        "randomIP" | "randomIPV4" => {
            Some(format!("{}.{}.{}.{}", rng.gen_range(1..255), rng.gen_range(0..255),
                         rng.gen_range(0..255), rng.gen_range(1..255)))
        }
        "randomIPV6" => {
            let segments: Vec<String> = (0..8).map(|_| format!("{:04x}", rng.gen::<u16>())).collect();
            Some(segments.join(":"))
        }
        "randomMACAddress" => {
            let octets: Vec<String> = (0..6).map(|_| format!("{:02x}", rng.gen::<u8>())).collect();
            Some(octets.join(":"))
        }
        "randomPassword" => {
            let charset: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
            let pw: String = (0..15).map(|_| charset[rng.gen_range(0..charset.len())] as char).collect();
            Some(pw)
        }
        "randomLocale" => {
            let locales = ["en", "fr", "de", "es", "it", "pt", "nl", "ja", "ko", "zh",
                           "ru", "ar", "hi", "sv", "no", "da", "fi", "pl", "cs", "tr"];
            Some(locales[rng.gen_range(0..locales.len())].to_string())
        }
        "randomUserAgent" => {
            let agents = [
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15",
                "Mozilla/5.0 (X11; Linux x86_64; rv:120.0) Gecko/20100101 Firefox/120.0",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:120.0) Gecko/20100101 Firefox/120.0",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            ];
            Some(agents[rng.gen_range(0..agents.len())].to_string())
        }
        "randomProtocol" => Some(if rng.gen_bool(0.5) { "http" } else { "https" }.to_string()),
        "randomSemver" => Some(format!("{}.{}.{}", rng.gen_range(0..10), rng.gen_range(0..10), rng.gen_range(0..10))),
        "randomDomainName" => {
            let word: String = fake::faker::lorem::en::Word().fake_with_rng(&mut rng);
            let tlds = [".com", ".org", ".net", ".io", ".dev"];
            Some(format!("{}{}", word, tlds[rng.gen_range(0..tlds.len())]))
        }
        "randomDomainSuffix" => {
            let suffixes = ["com", "org", "net", "io", "dev", "co", "info", "biz"];
            Some(suffixes[rng.gen_range(0..suffixes.len())].to_string())
        }
        "randomDomainWord" => {
            let word: String = fake::faker::lorem::en::Word().fake_with_rng(&mut rng);
            Some(word)
        }
        "randomExampleEmail" => {
            let first: String = fake::faker::name::en::FirstName().fake_with_rng(&mut rng);
            let last: String = fake::faker::name::en::LastName().fake_with_rng(&mut rng);
            Some(format!("{}.{}@example.com", first.to_lowercase(), last.to_lowercase()))
        }
        "randomEmail" => {
            let email: String = fake::faker::internet::en::FreeEmail().fake_with_rng(&mut rng);
            Some(email)
        }
        "randomUserName" => {
            let first: String = fake::faker::name::en::FirstName().fake_with_rng(&mut rng);
            Some(format!("{}{}", first.to_lowercase(), rng.gen_range(10..999)))
        }
        "randomUrl" => {
            let word: String = fake::faker::lorem::en::Word().fake_with_rng(&mut rng);
            let tlds = ["com", "org", "net", "io"];
            Some(format!("https://{}.{}", word, tlds[rng.gen_range(0..tlds.len())]))
        }

        // ── Names and Personal Information ──
        "randomFirstName" => Some(fake::faker::name::en::FirstName().fake_with_rng(&mut rng)),
        "randomLastName" => Some(fake::faker::name::en::LastName().fake_with_rng(&mut rng)),
        "randomFullName" => Some(fake::faker::name::en::Name().fake_with_rng(&mut rng)),
        "randomNamePrefix" => {
            let prefixes = ["Mr.", "Mrs.", "Ms.", "Dr.", "Prof."];
            Some(prefixes[rng.gen_range(0..prefixes.len())].to_string())
        }
        "randomNameSuffix" => {
            let suffixes = ["Jr.", "Sr.", "I", "II", "III", "IV", "V", "MD", "DDS", "PhD", "DVM"];
            Some(suffixes[rng.gen_range(0..suffixes.len())].to_string())
        }
        "randomJobArea" => {
            let areas = ["Marketing", "Engineering", "Sales", "Finance", "Human Resources",
                         "Operations", "Research", "Design", "Legal", "Support"];
            Some(areas[rng.gen_range(0..areas.len())].to_string())
        }
        "randomJobDescriptor" => {
            let descriptors = ["Senior", "Lead", "Junior", "Principal", "Chief", "Staff",
                               "Associate", "Executive", "Regional", "Global"];
            Some(descriptors[rng.gen_range(0..descriptors.len())].to_string())
        }
        "randomJobTitle" => {
            let title: String = fake::faker::job::en::Title().fake_with_rng(&mut rng);
            Some(title)
        }
        "randomJobType" => {
            let types = ["Full-time", "Part-time", "Contract", "Freelance", "Intern", "Temporary"];
            Some(types[rng.gen_range(0..types.len())].to_string())
        }
        "randomPhoneNumber" => {
            let phone: String = fake::faker::phone_number::en::PhoneNumber().fake_with_rng(&mut rng);
            Some(phone)
        }
        "randomPhoneNumberExt" => {
            let phone: String = fake::faker::phone_number::en::PhoneNumber().fake_with_rng(&mut rng);
            Some(format!("{} ext. {}", phone, rng.gen_range(100..999)))
        }

        // ── Location ──
        "randomCity" => Some(fake::faker::address::en::CityName().fake_with_rng(&mut rng)),
        "randomStreetName" => Some(fake::faker::address::en::StreetName().fake_with_rng(&mut rng)),
        "randomStreetAddress" => {
            let num = rng.gen_range(1..9999);
            let street: String = fake::faker::address::en::StreetName().fake_with_rng(&mut rng);
            Some(format!("{} {}", num, street))
        }
        "randomCountry" => Some(fake::faker::address::en::CountryName().fake_with_rng(&mut rng)),
        "randomCountryCode" => Some(fake::faker::address::en::CountryCode().fake_with_rng(&mut rng)),
        "randomLatitude" => Some(fake::faker::address::en::Latitude().fake_with_rng::<f64, _>(&mut rng).to_string()),
        "randomLongitude" => Some(fake::faker::address::en::Longitude().fake_with_rng::<f64, _>(&mut rng).to_string()),

        // ── Images ──
        "randomAvatarImage" => Some(format!("https://i.pravatar.cc/{}", rng.gen_range(200..400))),
        "randomImageUrl" => Some(format!("https://picsum.photos/{}/{}", rng.gen_range(200..800), rng.gen_range(200..800))),
        "randomAbstractImage" => Some("https://loremflickr.com/320/240/abstract".to_string()),
        "randomAnimalsImage" => Some("https://loremflickr.com/320/240/animals".to_string()),
        "randomBusinessImage" => Some("https://loremflickr.com/320/240/business".to_string()),
        "randomCatsImage" => Some("https://loremflickr.com/320/240/cats".to_string()),
        "randomCityImage" => Some("https://loremflickr.com/320/240/city".to_string()),
        "randomFoodImage" => Some("https://loremflickr.com/320/240/food".to_string()),
        "randomNightlifeImage" => Some("https://loremflickr.com/320/240/nightlife".to_string()),
        "randomFashionImage" => Some("https://loremflickr.com/320/240/fashion".to_string()),
        "randomPeopleImage" => Some("https://loremflickr.com/320/240/people".to_string()),
        "randomNatureImage" => Some("https://loremflickr.com/320/240/nature".to_string()),
        "randomSportsImage" => Some("https://loremflickr.com/320/240/sports".to_string()),
        "randomTransportImage" => Some("https://loremflickr.com/320/240/transport".to_string()),
        "randomImageDataUri" => {
            // Tiny 1x1 red PNG.
            Some("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==".to_string())
        }

        // ── Finance ──
        "randomBankAccount" => {
            let acct: String = (0..10).map(|_| rng.gen_range(b'0'..=b'9') as char).collect();
            Some(acct)
        }
        "randomBankAccountName" => {
            let names = ["Checking Account", "Savings Account", "Money Market Account",
                         "Investment Account", "Personal Loan Account", "Auto Loan Account"];
            Some(names[rng.gen_range(0..names.len())].to_string())
        }
        "randomCreditCardMask" => {
            Some(format!("**** **** **** {}{}{}{}", rng.gen_range(0..=9), rng.gen_range(0..=9),
                         rng.gen_range(0..=9), rng.gen_range(0..=9)))
        }
        "randomBankAccountBic" => {
            let charset: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
            let bic: String = (0..8).map(|_| charset[rng.gen_range(0..charset.len())] as char).collect();
            Some(bic)
        }
        "randomBankAccountIban" => {
            let countries = ["DE", "GB", "FR", "NL", "ES", "IT"];
            let country = countries[rng.gen_range(0..countries.len())];
            let digits: String = (0..18).map(|_| rng.gen_range(b'0'..=b'9') as char).collect();
            Some(format!("{}{}{}", country, rng.gen_range(10..99), digits))
        }
        "randomTransactionType" => {
            let types = ["deposit", "withdrawal", "payment", "invoice", "transfer"];
            Some(types[rng.gen_range(0..types.len())].to_string())
        }
        "randomCurrencyCode" => {
            let codes = ["USD", "EUR", "GBP", "JPY", "CAD", "AUD", "CHF", "CNY", "INR", "BRL"];
            Some(codes[rng.gen_range(0..codes.len())].to_string())
        }
        "randomCurrencyName" => {
            let names = ["US Dollar", "Euro", "British Pound", "Japanese Yen", "Canadian Dollar",
                         "Australian Dollar", "Swiss Franc", "Chinese Yuan", "Indian Rupee", "Brazilian Real"];
            Some(names[rng.gen_range(0..names.len())].to_string())
        }
        "randomCurrencySymbol" => {
            let symbols = ["$", "€", "£", "¥", "₹", "₿", "₩", "₽", "₺", "R$"];
            Some(symbols[rng.gen_range(0..symbols.len())].to_string())
        }
        "randomBitcoin" => {
            let charset: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
            let len = rng.gen_range(24..=33);
            let addr: String = (0..len).map(|_| charset[rng.gen_range(0..charset.len())] as char).collect();
            Some(format!("1{}", addr))
        }

        // ── Business ──
        "randomCompanyName" => Some(fake::faker::company::en::CompanyName().fake_with_rng(&mut rng)),
        "randomCompanySuffix" => Some(fake::faker::company::en::CompanySuffix().fake_with_rng(&mut rng)),
        "randomBs" => Some(fake::faker::company::en::Bs().fake_with_rng(&mut rng)),
        "randomBsAdjective" => Some(fake::faker::company::en::BsAdj().fake_with_rng(&mut rng)),
        "randomBsBuzz" => Some(fake::faker::company::en::BsVerb().fake_with_rng(&mut rng)),
        "randomBsNoun" => Some(fake::faker::company::en::BsNoun().fake_with_rng(&mut rng)),
        "randomCatchPhrase" => Some(fake::faker::company::en::CatchPhrase().fake_with_rng(&mut rng)),
        "randomCatchPhraseAdjective" => Some(fake::faker::company::en::CatchPhrase().fake_with_rng::<String, _>(&mut rng).split(' ').next().unwrap_or("innovative").to_string()),
        "randomCatchPhraseDescriptor" => {
            let descriptors = ["next-generation", "cutting-edge", "state-of-the-art", "best-in-class",
                               "mission-critical", "enterprise-grade", "high-performance", "cloud-native"];
            Some(descriptors[rng.gen_range(0..descriptors.len())].to_string())
        }
        "randomCatchPhraseNoun" => {
            let nouns = ["solutions", "platform", "infrastructure", "paradigm", "framework",
                         "architecture", "interface", "protocol", "middleware", "portal"];
            Some(nouns[rng.gen_range(0..nouns.len())].to_string())
        }

        // ── Database ──
        "randomDatabaseColumn" => {
            let cols = ["id", "name", "email", "created_at", "updated_at", "status",
                        "title", "description", "amount", "quantity", "price", "is_active"];
            Some(cols[rng.gen_range(0..cols.len())].to_string())
        }
        "randomDatabaseType" => {
            let types = ["varchar", "int", "bigint", "text", "boolean", "timestamp",
                         "decimal", "float", "uuid", "jsonb", "date", "bytea"];
            Some(types[rng.gen_range(0..types.len())].to_string())
        }
        "randomDatabaseCollation" => {
            let collations = ["utf8_general_ci", "utf8mb4_unicode_ci", "latin1_swedish_ci",
                              "utf8_unicode_ci", "ascii_general_ci", "utf8mb4_general_ci"];
            Some(collations[rng.gen_range(0..collations.len())].to_string())
        }
        "randomDatabaseEngine" => {
            let engines = ["InnoDB", "MyISAM", "MEMORY", "CSV", "ARCHIVE", "MERGE", "NDB"];
            Some(engines[rng.gen_range(0..engines.len())].to_string())
        }

        // ── Dates ──
        "randomDateFuture" => {
            let days = rng.gen_range(1..365);
            let future = chrono::Utc::now() + chrono::Duration::days(days);
            Some(future.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        }
        "randomDatePast" => {
            let days = rng.gen_range(1..365);
            let past = chrono::Utc::now() - chrono::Duration::days(days);
            Some(past.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        }
        "randomDateRecent" => {
            let hours = rng.gen_range(1..72);
            let recent = chrono::Utc::now() - chrono::Duration::hours(hours);
            Some(recent.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        }
        "randomWeekday" => {
            let days = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
            Some(days[rng.gen_range(0..days.len())].to_string())
        }
        "randomMonth" => {
            let months = ["January", "February", "March", "April", "May", "June",
                          "July", "August", "September", "October", "November", "December"];
            Some(months[rng.gen_range(0..months.len())].to_string())
        }

        // ── Files and System ──
        "randomFileName" => {
            let name: String = fake::faker::lorem::en::Word().fake_with_rng(&mut rng);
            let exts = ["pdf", "txt", "doc", "jpg", "png", "csv", "json", "xml", "html", "md"];
            Some(format!("{}.{}", name, exts[rng.gen_range(0..exts.len())]))
        }
        "randomFileType" => {
            let types = ["application/pdf", "image/jpeg", "text/plain", "application/json",
                         "image/png", "text/html", "application/xml", "text/csv"];
            Some(types[rng.gen_range(0..types.len())].to_string())
        }
        "randomFileExt" => {
            let exts = ["pdf", "txt", "doc", "jpg", "png", "csv", "json", "xml", "html", "zip"];
            Some(exts[rng.gen_range(0..exts.len())].to_string())
        }
        "randomCommonFileName" => {
            let name: String = fake::faker::lorem::en::Word().fake_with_rng(&mut rng);
            let exts = ["pdf", "jpg", "png", "doc", "txt"];
            Some(format!("{}.{}", name, exts[rng.gen_range(0..exts.len())]))
        }
        "randomCommonFileType" => {
            let types = ["application", "image", "text", "video", "audio"];
            Some(types[rng.gen_range(0..types.len())].to_string())
        }
        "randomCommonFileExt" => {
            let exts = ["pdf", "jpg", "png", "gif", "txt", "mp4", "mp3"];
            Some(exts[rng.gen_range(0..exts.len())].to_string())
        }
        "randomFilePath" => {
            let dirs = ["usr", "home", "var", "tmp", "opt", "etc"];
            let sub: String = fake::faker::lorem::en::Word().fake_with_rng(&mut rng);
            let name: String = fake::faker::lorem::en::Word().fake_with_rng(&mut rng);
            let exts = ["txt", "log", "conf", "json", "yml"];
            Some(format!("/{}/{}/{}.{}", dirs[rng.gen_range(0..dirs.len())], sub, name, exts[rng.gen_range(0..exts.len())]))
        }
        "randomDirectoryPath" => {
            let dirs = ["usr", "home", "var", "tmp", "opt", "etc"];
            let sub: String = fake::faker::lorem::en::Word().fake_with_rng(&mut rng);
            Some(format!("/{}/{}", dirs[rng.gen_range(0..dirs.len())], sub))
        }
        "randomMimeType" => {
            let mimes = ["application/json", "application/xml", "text/html", "text/plain",
                         "image/jpeg", "image/png", "application/pdf", "application/octet-stream",
                         "multipart/form-data", "application/x-www-form-urlencoded"];
            Some(mimes[rng.gen_range(0..mimes.len())].to_string())
        }

        // ── Commerce ──
        "randomPrice" => Some(format!("{:.2}", rng.gen_range(1.0_f64..1000.0))),
        "randomProduct" => {
            let products = ["Chair", "Table", "Keyboard", "Mouse", "Monitor", "Phone",
                            "Laptop", "Headphones", "Camera", "Watch", "Shoes", "Bag"];
            Some(products[rng.gen_range(0..products.len())].to_string())
        }
        "randomProductAdjective" => {
            let adjs = ["Ergonomic", "Modern", "Sleek", "Rustic", "Handmade",
                        "Refined", "Practical", "Gorgeous", "Incredible", "Fantastic"];
            Some(adjs[rng.gen_range(0..adjs.len())].to_string())
        }
        "randomProductMaterial" => {
            let materials = ["Wood", "Metal", "Plastic", "Leather", "Cotton",
                             "Rubber", "Steel", "Granite", "Concrete", "Silk"];
            Some(materials[rng.gen_range(0..materials.len())].to_string())
        }
        "randomProductName" => {
            let adjs = ["Ergonomic", "Modern", "Sleek", "Rustic", "Handmade"];
            let materials = ["Wood", "Metal", "Plastic", "Leather", "Cotton"];
            let products = ["Chair", "Table", "Keyboard", "Mouse", "Monitor"];
            Some(format!("{} {} {}",
                adjs[rng.gen_range(0..adjs.len())],
                materials[rng.gen_range(0..materials.len())],
                products[rng.gen_range(0..products.len())]))
        }
        "randomDepartment" => {
            let depts = ["Electronics", "Clothing", "Books", "Home", "Sports",
                         "Toys", "Garden", "Health", "Beauty", "Automotive"];
            Some(depts[rng.gen_range(0..depts.len())].to_string())
        }

        // ── Hacker and Lorem ──
        "randomNoun" => {
            let nouns = ["protocol", "interface", "system", "array", "bus", "port",
                         "driver", "firewall", "bandwidth", "pixel", "matrix", "capacitor"];
            Some(nouns[rng.gen_range(0..nouns.len())].to_string())
        }
        "randomVerb" => {
            let verbs = ["hack", "override", "parse", "reboot", "compress", "generate",
                         "quantify", "calculate", "synthesize", "transmit", "program", "navigate"];
            Some(verbs[rng.gen_range(0..verbs.len())].to_string())
        }
        "randomIngverb" => {
            let verbs = ["hacking", "overriding", "parsing", "rebooting", "compressing",
                         "generating", "quantifying", "calculating", "synthesizing", "transmitting"];
            Some(verbs[rng.gen_range(0..verbs.len())].to_string())
        }
        "randomAdjective" => {
            let adjs = ["digital", "virtual", "wireless", "neural", "optical", "haptic",
                        "auxiliary", "primary", "redundant", "mobile", "bluetooth", "solid state"];
            Some(adjs[rng.gen_range(0..adjs.len())].to_string())
        }
        "randomPhrase" => {
            let verbs = ["hack", "override", "parse", "reboot", "compress", "generate"];
            let adjs = ["digital", "virtual", "wireless", "neural", "optical", "haptic"];
            let nouns = ["protocol", "interface", "system", "array", "bus", "port"];
            Some(format!("Try to {} the {} {}",
                verbs[rng.gen_range(0..verbs.len())],
                adjs[rng.gen_range(0..adjs.len())],
                nouns[rng.gen_range(0..nouns.len())]))
        }
        "randomLoremWord" => {
            let word: String = fake::faker::lorem::en::Word().fake_with_rng(&mut rng);
            Some(word)
        }
        "randomLoremWords" => {
            let words: Vec<String> = fake::faker::lorem::en::Words(3..5).fake_with_rng(&mut rng);
            Some(words.join(" "))
        }
        "randomLoremSentence" => {
            let sentence: String = fake::faker::lorem::en::Sentence(5..10).fake_with_rng(&mut rng);
            Some(sentence)
        }
        "randomLoremSentences" => {
            let sentences: Vec<String> = (0..rng.gen_range(2..4))
                .map(|_| fake::faker::lorem::en::Sentence(5..10).fake_with_rng(&mut rng))
                .collect();
            Some(sentences.join(" "))
        }
        "randomLoremParagraph" => {
            let para: String = fake::faker::lorem::en::Paragraph(3..5).fake_with_rng(&mut rng);
            Some(para)
        }
        "randomLoremParagraphs" => {
            let paras: Vec<String> = (0..rng.gen_range(2..4))
                .map(|_| fake::faker::lorem::en::Paragraph(3..5).fake_with_rng(&mut rng))
                .collect();
            Some(paras.join("\n\n"))
        }
        "randomLoremText" => {
            let para: String = fake::faker::lorem::en::Paragraph(3..5).fake_with_rng(&mut rng);
            Some(para)
        }
        "randomLoremSlug" => {
            let words: Vec<String> = fake::faker::lorem::en::Words(3..4).fake_with_rng(&mut rng);
            Some(words.join("-"))
        }
        "randomLoremLines" => {
            let lines: Vec<String> = (0..rng.gen_range(2..5))
                .map(|_| fake::faker::lorem::en::Sentence(4..8).fake_with_rng(&mut rng))
                .collect();
            Some(lines.join("\n"))
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All known dynamic variable names — must match Bruno's complete list.
    const ALL_DYNAMIC_VARS: &[&str] = &[
        // Basic Data Types
        "guid", "timestamp", "isoTimestamp", "randomUUID", "randomNanoId",
        "randomAlphaNumeric", "randomBoolean", "randomInt", "randomColor",
        "randomHexColor", "randomAbbreviation", "randomWord", "randomWords",
        // Internet and Network
        "randomIP", "randomIPV4", "randomIPV6", "randomMACAddress", "randomPassword",
        "randomLocale", "randomUserAgent", "randomProtocol", "randomSemver",
        "randomDomainName", "randomDomainSuffix", "randomDomainWord",
        "randomExampleEmail", "randomEmail", "randomUserName", "randomUrl",
        // Names
        "randomFirstName", "randomLastName", "randomFullName", "randomNamePrefix",
        "randomNameSuffix", "randomJobArea", "randomJobDescriptor", "randomJobTitle",
        "randomJobType", "randomPhoneNumber", "randomPhoneNumberExt",
        // Location
        "randomCity", "randomStreetName", "randomStreetAddress", "randomCountry",
        "randomCountryCode", "randomLatitude", "randomLongitude",
        // Images
        "randomAvatarImage", "randomImageUrl", "randomAbstractImage",
        "randomAnimalsImage", "randomBusinessImage", "randomCatsImage",
        "randomCityImage", "randomFoodImage", "randomNightlifeImage",
        "randomFashionImage", "randomPeopleImage", "randomNatureImage",
        "randomSportsImage", "randomTransportImage", "randomImageDataUri",
        // Finance
        "randomBankAccount", "randomBankAccountName", "randomCreditCardMask",
        "randomBankAccountBic", "randomBankAccountIban", "randomTransactionType",
        "randomCurrencyCode", "randomCurrencyName", "randomCurrencySymbol", "randomBitcoin",
        // Business
        "randomCompanyName", "randomCompanySuffix", "randomBs", "randomBsAdjective",
        "randomBsBuzz", "randomBsNoun", "randomCatchPhrase", "randomCatchPhraseAdjective",
        "randomCatchPhraseDescriptor", "randomCatchPhraseNoun",
        // Database
        "randomDatabaseColumn", "randomDatabaseType", "randomDatabaseCollation", "randomDatabaseEngine",
        // Dates
        "randomDateFuture", "randomDatePast", "randomDateRecent", "randomWeekday", "randomMonth",
        // Files
        "randomFileName", "randomFileType", "randomFileExt", "randomCommonFileName",
        "randomCommonFileType", "randomCommonFileExt", "randomFilePath",
        "randomDirectoryPath", "randomMimeType",
        // Commerce
        "randomPrice", "randomProduct", "randomProductAdjective", "randomProductMaterial",
        "randomProductName", "randomDepartment",
        // Hacker and Lorem
        "randomNoun", "randomVerb", "randomIngverb", "randomAdjective", "randomPhrase",
        "randomLoremWord", "randomLoremWords", "randomLoremSentence", "randomLoremSentences",
        "randomLoremParagraph", "randomLoremParagraphs", "randomLoremText",
        "randomLoremSlug", "randomLoremLines",
    ];

    #[test]
    fn all_registered_names_generate_values() {
        for name in ALL_DYNAMIC_VARS {
            assert!(
                generate(name).is_some(),
                "dynamic var '{}' returned None — missing from match",
                name
            );
        }
    }

    #[test]
    fn unknown_name_returns_none() {
        assert!(generate("doesNotExist").is_none());
        assert!(generate("").is_none());
        assert!(generate("GUID").is_none()); // case-sensitive
    }

    #[test]
    fn is_dynamic_var_matches_generate() {
        for name in ALL_DYNAMIC_VARS {
            assert!(is_dynamic_var(name), "'{}' should be recognized", name);
        }
        assert!(!is_dynamic_var("unknownThing"));
    }

    #[test]
    fn guid_is_valid_uuid() {
        let val = generate("guid").unwrap();
        assert!(uuid::Uuid::parse_str(&val).is_ok(), "guid '{}' is not valid UUID", val);
    }

    #[test]
    fn random_uuid_is_valid_uuid() {
        let val = generate("randomUUID").unwrap();
        assert!(uuid::Uuid::parse_str(&val).is_ok(), "randomUUID '{}' is not valid UUID", val);
    }

    #[test]
    fn timestamp_is_valid_i64() {
        let val = generate("timestamp").unwrap();
        assert!(val.parse::<i64>().is_ok(), "timestamp '{}' is not valid i64", val);
    }

    #[test]
    fn iso_timestamp_is_valid_rfc3339() {
        let val = generate("isoTimestamp").unwrap();
        assert!(chrono::DateTime::parse_from_rfc3339(&val).is_ok(),
            "isoTimestamp '{}' is not valid RFC 3339", val);
    }

    #[test]
    fn random_int_in_range() {
        for _ in 0..20 {
            let val: i32 = generate("randomInt").unwrap().parse().unwrap();
            assert!((0..=1000).contains(&val), "randomInt {} out of range", val);
        }
    }

    #[test]
    fn random_boolean_valid() {
        for _ in 0..20 {
            let val = generate("randomBoolean").unwrap();
            assert!(val == "true" || val == "false", "randomBoolean '{}' invalid", val);
        }
    }

    #[test]
    fn two_calls_produce_different_values() {
        // Probabilistic: 10 GUIDs should not all be identical
        let vals: Vec<String> = (0..10).map(|_| generate("guid").unwrap()).collect();
        let first = &vals[0];
        assert!(vals.iter().any(|v| v != first), "10 guid calls all returned same value");
    }
}
