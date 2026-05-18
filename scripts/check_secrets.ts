import fs from 'fs';
import path from 'path';

console.log("=== بدء التحقق من منع تسريب المفاتيح والرموز السرية ===");

const FORBIDDEN_PATTERNS = [
  { regex: /AGE-SECRET-KEY-1[a-zA-Z0-9]{58}/i, label: 'Committed AGE Secret Key' },
  { regex: /"admin"\s*:\s*"admin"/i, label: 'Committed admin/admin credentials JSON' },
  { regex: /username\s*==\s*["']admin["']\s*&&\s*password\s*==\s*["']admin["']/i, label: 'Committed hardcoded admin credentials comparison' },
  { regex: /const\s+SECRET\s*=\s*["'][a-fA-F0-9]{64}["']/i, label: 'Embedded hex signing key' }
];

let failed = false;

// تحقق من عدم وجود ملفات بيئة نشطة تم إيداعها
const ENV_FILES = ['.env', '.env.local', '.env.production', '.env.development'];
for (const envFile of ENV_FILES) {
  const envPath = path.resolve(process.cwd(), envFile);
  if (fs.existsSync(envPath)) {
    // تحقق مما إذا كانت مدارة في git
    try {
      const gitCheck = require('child_process').execSync(`git ls-files --error-unmatch ${envFile}`, { stdio: 'pipe' });
      if (gitCheck.toString().trim()) {
        console.error(`❌ خطأ أمني جسيم: تم العثور على ملف البيئة [${envFile}] مُداراً ومرفوعاً في Git!`);
        failed = true;
      }
    } catch (e) {
      // ليس تحت تعقب git، وهذا صحيح
    }
  }
}

// فحص محتويات الملفات في المجلدات البرمجية
const SCAN_DIRS = ['src', 'src-tauri/src', 'scripts'];
const SCAN_EXTENSIONS = ['.ts', '.js', '.svelte', '.rs', '.sql', '.toml'];

function scanDirectory(dirPath: string) {
  if (!fs.existsSync(dirPath)) return;
  
  const files = fs.readdirSync(dirPath);
  for (const file of files) {
    const fullPath = path.join(dirPath, file);
    const stat = fs.statSync(fullPath);
    
    if (stat.isDirectory()) {
      // تجاهل المجلدات غير المرغوبة
      if (['node_modules', 'target', 'dist', '.git', 'playwright-report', 'test-results'].includes(file)) continue;
      scanDirectory(fullPath);
    } else {
      const ext = path.extname(file);
      if (SCAN_EXTENSIONS.includes(ext)) {
        // تجنب فحص كود الفحص ذاته لمنع التعارض مع النماذج المحظورة المعرفة أعلاه
        if (file === 'check_secrets.ts') continue;
        
        try {
          const content = fs.readFileSync(fullPath, 'utf8');
          for (const pattern of FORBIDDEN_PATTERNS) {
            const matches = content.match(pattern.regex);
            if (matches) {
              if (pattern.label === 'Committed AGE Secret Key') {
                const isOnlyMock = matches.every(m => m.toUpperCase() === 'AGE-SECRET-KEY-1KTYK6RVLN5TAPE7VF6FQQSKZ9HWWCDSKUGXXNUQDWZ7XXT5YK5LSF3UTKQ');
                if (isOnlyMock) continue;
              }
              console.error(`❌ خطأ أمني جسيم: تم الكشف عن [${pattern.label}] في الملف: ${path.relative(process.cwd(), fullPath)}`);
              failed = true;
            }
          }
        } catch (err) {
          // خطأ في القراءة
        }
      }
    }
  }
}

for (const dir of SCAN_DIRS) {
  scanDirectory(path.resolve(process.cwd(), dir));
}

if (failed) {
  console.error("\n❌ فشل تدقيق السرية والأمان! يرجى إزالة المفاتيح والرموز المذكورة أعلاه.");
  process.exit(1);
} else {
  console.log("\n✅ نجح تدقيق الأمان والسرية بالكامل بنسبة 100%!");
  process.exit(0);
}
