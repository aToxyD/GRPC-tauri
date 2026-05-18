import fs from 'fs';
import path from 'path';

console.log("=== بدء تدقيق حوكمة الوثائق التقنية ===");

const REQUIRED_FILES = [
  'docs/architecture/ARCHITECTURAL_INVARIANTS.md',
  'docs/security/THREAT_MODEL.md',
  'docs/technical/CLAIM_VERIFICATION_MATRIX.md',
  'docs/technical/DOCUMENTATION_GOVERNANCE.md',
  'docs/technical/OPERATIONAL_LIMITATIONS.md',
  'docs/technical/GLOSSARY.md',
];

const FORBIDDEN_PATTERNS = [
  { term: 'military-grade', label: 'Military-Grade' },
  { term: 'مستوى عسكري', label: 'مستوى عسكري' },
  { term: 'unbreakable', label: 'Unbreakable' },
  { term: 'غير قابل للكسر', label: 'غير قابل للكسر' },
  { term: 'impossible to hack', label: 'Impossible to Hack' },
  { term: 'مستحيل الاختراق', label: 'مستحيل الاختراق' },
  { term: 'zero-risk', label: 'Zero-Risk' },
  { term: 'خالٍ من المخاطر', label: 'خالٍ من المخاطر' },
  { term: 'tamper-proof', label: 'Tamper-Proof' },
  { term: 'مقاوم للتلاعب المطلق', label: 'مقاوم للتلاعب المطلق' },
  { term: 'perfectly secure', label: 'Perfectly Secure' },
  { term: 'آمن تماماً', label: 'آمن تماماً' }
];

let failed = false;

// 1. تحقق من وجود الملفات المطلوبة
console.log("\n1. التحقق من وجود الوثائق الإلزامية:");
for (const file of REQUIRED_FILES) {
  const absolutePath = path.resolve(process.cwd(), file);
  if (!fs.existsSync(absolutePath)) {
    console.error(`❌ خطأ: المستند الإلزامي غير موجود: ${file}`);
    failed = true;
  } else {
    console.log(`✅ موجود: ${file}`);
  }
}

// 2. التحقق من صلاحية UTF-8 وخلو الوثائق من العبارات الترويجية المحظورة
console.log("\n2. فحص سلامة UTF-8 والعبارات المحظورة:");
for (const file of REQUIRED_FILES) {
  const absolutePath = path.resolve(process.cwd(), file);
  if (!fs.existsSync(absolutePath)) continue;

  try {
    const rawBuffer = fs.readFileSync(absolutePath);
    // تأكيد صحة ترميز UTF-8
    const content = rawBuffer.toString('utf8');
    const reEncoded = Buffer.from(content, 'utf8');
    if (!rawBuffer.equals(reEncoded)) {
      console.error(`❌ خطأ: ترميز الملف ${file} ليس UTF-8 صالحاً!`);
      failed = true;
    }

    // البحث عن العبارات المحظورة (تخطي الملف الخاص بدليل الحوكمة لأنه يسردها كأمثلة محظورة)
    if (!file.endsWith('DOCUMENTATION_GOVERNANCE.md')) {
      for (const pattern of FORBIDDEN_PATTERNS) {
        if (content.toLowerCase().includes(pattern.term.toLowerCase())) {
          console.error(`❌ خطأ أمني: تم العثور على عبارة محظورة غير واقعية [${pattern.label}] في الملف: ${file}`);
          failed = true;
        }
      }
    }
  } catch (err) {
    console.error(`❌ فشل في فحص الملف ${file}:`, err);
    failed = true;
  }
}

// 3. التحقق من سلامة روابط ADRs والمراجع المعمارية
console.log("\n3. التحقق من صحة مراجع ADRs والروابط المعمارية:");
function scanForADRLinks(dirPath: string) {
  const files = fs.readdirSync(dirPath);
  for (const file of files) {
    const fullPath = path.join(dirPath, file);
    const stat = fs.statSync(fullPath);
    if (stat.isDirectory()) {
      scanForADRLinks(fullPath);
    } else if (file.endsWith('.md')) {
      const content = fs.readFileSync(fullPath, 'utf8');
      
      // روابط ماركداون للـ ADRs مثل docs/architecture/0001-...md
      const adrLinkRegex = /docs\/architecture\/(00\d{2}-[a-zA-Z0-9_\-\.]+)/g;
      let match;
      while ((match = adrLinkRegex.exec(content)) !== null) {
        const adrFile = match[1];
        const adrFilePath = path.resolve(process.cwd(), 'docs/architecture', adrFile);
        if (!fs.existsSync(adrFilePath)) {
          console.error(`❌ خطأ مرجعي: رابط ADR غير صالح [${adrFile}] في الملف: ${path.relative(process.cwd(), fullPath)}`);
          failed = true;
        }
      }
    }
  }
}

try {
  scanForADRLinks(path.resolve(process.cwd(), 'docs'));
  console.log("✅ انتهى التحقق من مراجع ADRs والروابط المعمارية بنجاح.");
} catch (err) {
  console.error("❌ فشل فحص روابط الـ ADRs:", err);
  failed = true;
}

if (failed) {
  console.error("\n❌ فشل تدقيق حوكمة الوثائق! يرجى إصلاح الأخطاء المذكورة أعلاه.");
  process.exit(1);
} else {
  console.log("\n✅ نجح تدقيق حوكمة الوثائق بالكامل بنسبة 100%!");
  process.exit(0);
}
