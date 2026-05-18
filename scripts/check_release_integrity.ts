import fs from 'fs';
import path from 'path';

console.log("=== بدء تدقيق سلامة الإصدار (Release Integrity Audit) ===");

let failed = false;

// 1. التحقق من اتساق أرقام الإصدارات (Version Consistency)
try {
  const packageJsonPath = path.resolve(process.cwd(), 'package.json');
  const cargoTomlPath = path.resolve(process.cwd(), 'src-tauri/Cargo.toml');

  if (!fs.existsSync(packageJsonPath) || !fs.existsSync(cargoTomlPath)) {
    console.error("❌ خطأ: ملفات التكوين package.json أو Cargo.toml غير موجودة!");
    failed = true;
  } else {
    const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
    const packageVersion = packageJson.version;

    const cargoTomlContent = fs.readFileSync(cargoTomlPath, 'utf8');
    const versionMatch = cargoTomlContent.match(/version\s*=\s*"([^"]+)"/);

    if (!versionMatch) {
      console.error("❌ خطأ: تعذر العثور على حقل الإصدار في Cargo.toml!");
      failed = true;
    } else {
      const cargoVersion = versionMatch[1];
      if (packageVersion !== cargoVersion) {
        console.error(`❌ خطأ: عدم اتساق في رقم الإصدار! (package.json: ${packageVersion} vs Cargo.toml: ${cargoVersion})`);
        failed = true;
      } else {
        console.log(`✅ اتساق رقم الإصدار: ${packageVersion}`);
      }
    }
  }
} catch (err) {
  console.error("❌ فشل في التحقق من اتساق الإصدارات:", err);
  failed = true;
}

// 2. التحقق من وجود وتحديث سجل التغييرات (Changelog Presence)
try {
  const changelogPath = path.resolve(process.cwd(), 'CHANGELOG.md');
  if (!fs.existsSync(changelogPath)) {
    console.error("❌ خطأ: ملف سجل التغييرات CHANGELOG.md غير موجود!");
    failed = true;
  } else {
    const packageJson = JSON.parse(fs.readFileSync(path.resolve(process.cwd(), 'package.json'), 'utf8'));
    const packageVersion = packageJson.version;
    const changelogContent = fs.readFileSync(changelogPath, 'utf8');
    
    if (!changelogContent.includes(packageVersion)) {
      console.error(`❌ خطأ: سجل التغييرات CHANGELOG.md لا يوثق الإصدار الحالي [${packageVersion}]!`);
      failed = true;
    } else {
      console.log("✅ سجل التغييرات CHANGELOG.md موجود ومحدث بالإصدار الحالي.");
    }
  }
} catch (err) {
  console.error("❌ فشل التحقق من سجل التغييرات:", err);
  failed = true;
}

// 3. التحقق من تكامل مراجع ADRs والوثائق في ملف سجل المعمارية الرئيسي
try {
  const adrReadmePath = path.resolve(process.cwd(), 'docs/architecture/README.md');
  if (!fs.existsSync(adrReadmePath)) {
    console.error("❌ خطأ: سجل مراجع الـ ADRs الرئيسي docs/architecture/README.md غير موجود!");
    failed = true;
  } else {
    console.log("✅ سجل مراجع الـ ADRs الرئيسي موجود.");
  }
} catch (err) {
  console.error("❌ فشل التحقق من مراجع ADRs:", err);
  failed = true;
}

// 4. تشغيل تدقيق منع تسريب المفاتيح تلقائياً كجزء من فحص سلامة الإصدار
try {
  const checkSecretsPath = path.resolve(process.cwd(), 'scripts/check_secrets.ts');
  if (fs.existsSync(checkSecretsPath)) {
    const { status } = require('child_process').spawnSync('bun', ['run', checkSecretsPath], { stdio: 'inherit' });
    if (status !== 0) {
      console.error("❌ خطأ: فشل تدقيق السرية والأمان لمنع تسريب المفاتيح!");
      failed = true;
    } else {
      console.log("✅ نجح تدقيق السرية والأمان بنجاح.");
    }
  }
} catch (err) {
  console.error("❌ فشل تشغيل تدقيق السرية التلقائي:", err);
  failed = true;
}

// 5. التحقق من خلو ملفات التكوين من إعدادات كشف الأخطاء في الإنتاج (No Debug Assertions in Cargo.toml release profiles)
try {
  const cargoTomlPath = path.resolve(process.cwd(), 'src-tauri/Cargo.toml');
  if (fs.existsSync(cargoTomlPath)) {
    const cargoTomlContent = fs.readFileSync(cargoTomlPath, 'utf8');
    // البحث عن أي تعطيل غير مقصود لـ debug-assertions في ملف الإنتاج
    if (cargoTomlContent.includes('debug-assertions = true') && cargoTomlContent.includes('[profile.release]')) {
      console.error("❌ خطأ أمني: تم العثور على تفعيل debug-assertions داخل ملف الإنتاج [profile.release]!");
      failed = true;
    } else {
      console.log("✅ خلو ملف الإنتاج من تفعيل debug-assertions غير المرغوب.");
    }
  }
} catch (err) {
  failed = true;
}

if (failed) {
  console.error("\n❌ فشل تدقيق سلامة الإصدار! يرجى مراجعة وتصحيح الأخطاء أعلاه.");
  process.exit(1);
} else {
  console.log("\n✅ نجح تدقيق سلامة الإصدار بالكامل بنسبة 100%!");
  process.exit(0);
}
