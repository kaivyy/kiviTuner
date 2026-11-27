/**
 * Structured workflow guide chapters and search/navigation helpers for the
 * interactive "How to Use kiviTuner" tutorial window.
 */

export type TutorialSceneId =
  | 'workspace'
  | 'explorer'
  | 'tables'
  | 'scalar-flag'
  | 'hex-reverse'
  | 'diff-checksum'
  | 'telemetry-ecu';

export interface TutorialCallout {
  pin: number;
  label: string;
  detail: string;
}

export interface TutorialShortcut {
  keys: string[];
  action: string;
}

export interface TutorialChapter {
  id: TutorialSceneId;
  index: number;
  shortTitle: string;
  title: string;
  subtitle: string;
  menuPath: string;
  figureCaption: string;
  figureAriaLabel: string;
  shortcuts: TutorialShortcut[];
  callouts: TutorialCallout[];
  steps: string[];
  tip: string;
  keywords: string[];
}

export const TUTORIAL_CHAPTERS: readonly TutorialChapter[] = [
  {
    id: 'workspace',
    index: 1,
    shortTitle: 'Mulai Cepat',
    title: 'Mulai Cepat & Navigasi Workspace',
    subtitle: 'Buka file ROM BIN, muat definisi XDF, lalu atur panel kerja utama dan tema.',
    menuPath: 'File -> Open ROM... / Definition (XDF) -> Load XDF...',
    figureCaption: 'Potongan atas Menu Bar Tahoe, Activity Rail kiri, dan Workspace Tab Bar.',
    figureAriaLabel:
      'Ilustrasi potongan workspace utama KiviTuner berisi Menu Bar, indikator Checksum, Activity Rail, dan tab peta.',
    shortcuts: [
      { keys: ['Ctrl', 'O'], action: 'Buka file ROM BIN' },
      { keys: ['Ctrl', 'D'], action: 'Muat file definisi XDF' },
      { keys: ['Ctrl', 'K'], action: 'Command Palette' },
    ],
    callouts: [
      {
        pin: 1,
        label: 'Menu Bar & Checksum Pill',
        detail: 'Menampung 7 menu utama serta indikator Checksum dan Delta byte aktif.',
      },
      {
        pin: 2,
        label: 'Activity Rail Kiri',
        detail: 'Beralih cepat ke Map Explorer, Inspector, Discovery Scanner, Hex, Waveform, dan tema Dark/Light.',
      },
      {
        pin: 3,
        label: 'Workspace Tabs & Split View',
        detail: 'Kelola dokumen ROM, pin tab peta aktif, atau buka dua peta berdampingan.',
      },
    ],
    steps: [
      'Pilih File -> Open ROM... (Ctrl+O) untuk memuat file biner ECU (.bin) ke ruang kerja.',
      'Pilih Definition (XDF) -> Load XDF... (Ctrl+D) atau file .kivi.json agar daftar peta muncul otomatis.',
      'Gunakan Activity Rail di sisi kiri untuk membuka atau menutup Map Explorer, Inspector, Hex, dan Waveform.',
      'Tekan Ctrl+K kapan saja untuk membuka Command Palette jika ingin menjalankan perintah tanpa menelusuri menu.',
    ],
    tip: 'Mode tema Carbon Dark dan Lab Light dapat ditukar dari tombol tema di Activity Rail atau menu View & Workspace.',
    keywords: ['open', 'rom', 'bin', 'xdf', 'tema', 'dark', 'light', 'command palette', 'workspace', 'tab'],
  },
  {
    id: 'explorer',
    index: 2,
    shortTitle: 'Map Explorer',
    title: 'Map Explorer, Filter Dimensi & Potential Maps',
    subtitle: 'Saring peta berdasarkan ukuran 2D, 1D, atau 1x1, dan buat definisi peta baru.',
    menuPath: 'Sidebar Kiri (Map Explorer) / Definition (XDF) -> Create New...',
    figureCaption: 'Potongan sidebar Map Explorer dengan filter dimensi dan folder Potential Maps.',
    figureAriaLabel:
      'Ilustrasi potongan Map Explorer menampilkan kolom pencarian, filter All 2D 1D 1x1, daftar peta, dan folder Potential Maps.',
    shortcuts: [
      { keys: ['Ctrl', 'F'], action: 'Jalankan Heuristic Auto-Scan' },
    ],
    callouts: [
      {
        pin: 1,
        label: 'Filter All / 2D / 1D / 1x1',
        detail: 'Menyaring daftar peta berdasarkan dimensi matriks 2D, 1D, atau skalar 1x1.',
      },
      {
        pin: 2,
        label: 'Tombol + Definisi Baru',
        detail: 'Membuat definisi Table 2D/3D, Axis 1D, Scalar 1x1, atau Bitmask Flag.',
      },
      {
        pin: 3,
        label: 'Folder Potential Maps',
        detail: 'Daftar kandidat peta hasil scan heuristik yang bisa diklik atau dipromosikan menjadi peta resmi.',
      },
    ],
    steps: [
      'Gunakan kotak pencarian untuk mencari nama parameter atau kategori peta dengan cepat.',
      'Klik pill All, 2D, 1D, atau 1x1 untuk menyaring tabel bahan bakar/pengapian, kurva, atau skalar tunggal.',
      'Klik ikon + di header Map Explorer (atau menu Definition (XDF)) untuk menambah peta baru secara manual.',
      'Buka folder lipat Potential Maps setelah menjalankan Auto-Scan guna meninjau kandidat tabel yang ditemukan.',
    ],
    tip: 'Klik kanan pada item peta di Explorer (atau gunakan tombol Split di toolbar) untuk menaruh peta pembanding di panel kedua.',
    keywords: ['explorer', 'filter', '2d', '1d', '1x1', 'potential maps', 'kategori', 'xdf', 'buat peta'],
  },
  {
    id: 'tables',
    index: 3,
    shortTitle: 'Tabel & Kurva',
    title: 'Edit Tabel 2D, Kurva 1D & Surface 3D',
    subtitle: 'Ubah nilai sel kalibrasi dengan operasi matematika cepat, smoothing, dan tampilan 3D.',
    menuPath: 'Edit & Table Math / View & Workspace -> Layout Presets',
    figureCaption: 'Potongan Heatmap Matrix 2D beserta toolbar operasi matematika dan pratinjau Surface 3D.',
    figureAriaLabel:
      'Ilustrasi potongan editor tabel 2D Heatmap lengkap dengan sel warna, seleksi blok, tombol math, dan Surface 3D.',
    shortcuts: [
      { keys: ['S'], action: 'Smooth 3x3 area terpilih' },
      { keys: ['I'], action: 'Interpolasi Bilinear 2D' },
      { keys: ['H'], action: 'Interpolasi Horizontal' },
      { keys: ['Ctrl', '\\'], action: 'Split 2D + 3D Surface' },
    ],
    callouts: [
      {
        pin: 1,
        label: 'Seleksi Blok Heatmap',
        detail: 'Klik dan seret beberapa sel untuk mengedit rentang beban dan RPM sekaligus.',
      },
      {
        pin: 2,
        label: 'Toolbar Table Math (+, -, %, =, S, I)',
        detail: 'Tambah/kurang konstanta, skalakan persentase, haluskan dengan Smooth 3x3, atau interpolasi antar tepi.',
      },
      {
        pin: 3,
        label: 'Split 2D + 3D / Kurva 1D',
        detail: 'Visualisasikan kontur perubahan secara spasial atau tarik titik kurva 1D secara langsung.',
      },
    ],
    steps: [
      'Pilih peta 2D atau 1D di Map Explorer untuk membukanya di area kerja utama.',
      'Seret kursor pada sel Heatmap lalu tekan +, -, %, atau = (atau buka menu Edit & Table Math) untuk mengubah nilai massal.',
      'Gunakan tombol S (Smooth 3x3) atau I / H / V (Interpolate) agar transisi antarsel tetap halus.',
      'Tekan Ctrl+\\ atau buka View & Workspace -> Layout Presets -> Split 2D+3D untuk memantau bentuk permukaan secara bersamaan.',
    ],
    tip: 'Peta berdimensi 1xN atau Nx1 otomatis menampilkan editor kurva 1D lengkap dengan garis referensi stok putus-putus.',
    keywords: ['tabel', 'heatmap', '2d', '3d', 'surface', 'kurva', '1d', 'smooth', 'interpolasi', 'math'],
  },
  {
    id: 'scalar-flag',
    index: 4,
    shortTitle: 'Scalar 1x1 & Flag',
    title: 'Editor Scalar 1x1 & Sakelar Bitmask Flag',
    subtitle: 'Atur rev limiter, konstanta tunggal, dan switch bit 8-bit/16-bit tanpa salah hitung hex.',
    menuPath: 'Map Explorer -> Filter 1x1 / Definition (XDF) -> Create 1x1 Scalar / Bitmask Flag',
    figureCaption: 'Potongan ScalarFlagEditor dengan pembacaan Decimal, Raw Hex, Binary, dan sakelar bitmask.',
    figureAriaLabel:
      'Ilustrasi potongan Scalar dan Bitmask Flag Editor menampilkan nilai desimal, hex, biner, delta baseline, serta tombol bit 0 sampai 7.',
    shortcuts: [
      { keys: ['Ctrl', 'Z'], action: 'Undo perubahan terakhir' },
      { keys: ['Ctrl', 'Y'], action: 'Redo perubahan' },
    ],
    callouts: [
      {
        pin: 1,
        label: 'Nilai Terkalibrasi, Hex & Biner',
        detail: 'Ketiga representasi (Engineering, Raw Hex, Binary) tersinkron otomatis saat diedit.',
      },
      {
        pin: 2,
        label: 'Sakelar Bitmask (8-Bit / 16-Bit)',
        detail: 'Klik setiap kotak bit untuk menyalakan atau mematikan flag ECU individual.',
      },
      {
        pin: 3,
        label: 'Baseline Delta & Grid View',
        detail: 'Memantau selisih terhadap stok dan berpindah ke tampilan sel standar satu kali klik.',
      },
    ],
    steps: [
      'Klik filter 1x1 di Map Explorer atau buat parameter baru melalui Create 1x1 Scalar / Create Bitmask Flag.',
      'Ubah angka utama secara langsung untuk nilai skalar (misal batas RPM atau target AFR).',
      'Untuk peta flag, klik kotak Bit 0..Bit 7 (atau 16-bit) guna mengubah status bit tanpa menghitung mask manual.',
      'Klik tombol Grid View di pojok kanan atas editor apabila Anda lebih memilih mode sel tunggal klasik.',
    ],
    tip: 'Perhatikan baris Stock Baseline dan Delta di kartu editor untuk memastikan selisih angka tetap sesuai target sebelum menyimpan.',
    keywords: ['scalar', '1x1', 'bitmask', 'flag', 'switch', 'bit', 'hex', 'binary', 'grid view'],
  },
  {
    id: 'hex-reverse',
    index: 5,
    shortTitle: 'Hex Pro & Ledger',
    title: 'Hexdump Pro & Change Ledger',
    subtitle: 'Inspeksi byte mentah 8b/16b, navigasi kandidat peta, dan pantau setiap perubahan ROM.',
    menuPath: 'Tab Workspace -> Hex / Inspector Sidebar -> Change Ledger (vs Original)',
    figureCaption: 'Potongan Hexdump Pro toolbar, sorotan Diff & Potential Maps, Data Inspector, dan Change Ledger.',
    figureAriaLabel:
      'Ilustrasi potongan Hexdump Pro menampilkan kontrol 8b 16b Hex Dec, Diff hijau, kandidat peta amber, Data Inspector, dan Change Ledger.',
    shortcuts: [
      { keys: ['W'], action: 'Persempit kolom Hex' },
      { keys: ['M'], action: 'Perlebar kolom Hex' },
      { keys: ['F'], action: 'Lompat ke Potential Map berikut' },
      { keys: ['Shift', 'F'], action: 'Potential Map sebelumnya' },
    ],
    callouts: [
      {
        pin: 1,
        label: 'Kontrol 8b/16b, Hex/Dec & Diff',
        detail: 'Atur lebar kata, format angka, jumlah kolom 8B..32B, dan sorot hijau byte yang berubah.',
      },
      {
        pin: 2,
        label: 'Navigasi < Maps > & Promote Map',
        detail: 'Lompat ke blok amber hasil Auto-Scan lalu pilih rentang byte untuk dijadikan peta lewat Promote Map.',
      },
      {
        pin: 3,
        label: 'Inspector Change Ledger',
        detail: 'Daftar klaster byte termodifikasi terhadap ROM asli; klik baris mana saja untuk melompat ke alamatnya di Hex.',
      },
    ],
    steps: [
      'Buka tab Hex di Workspace Tab Bar atau tekan ikon Hex di Activity Rail.',
      'Gunakan tombol 8b/16b, Hex/Dec, serta tombol -/+ kolom (atau hotkey W / M) agar pola struktur tabel terlihat jelas.',
      'Aktifkan tombol Diff untuk menandai byte yang berbeda dari ROM stok, dan tekan F / Shift+F untuk menelusuri Potential Maps.',
      'Seret rentang byte di hex grid (atau klik blok kandidat) lalu klik Promote Map untuk langsung membuat definisi tabel.',
    ],
    tip: 'Data Inspector di bagian bawah Hex viewer otomatis membaca nilai BE/LE u8, i8, u16, i16, u32, i32, hingga f32 pada kursor aktif.',
    keywords: ['hex', 'hexdump', 'hex editor', '8b', '16b', 'diff', 'promote map', 'change ledger', 'inspector', 'waveform'],
  },
  {
    id: 'diff-checksum',
    index: 6,
    shortTitle: 'Diff & Checksum',
    title: 'Binary Diff, Checksum & Simpan ROM',
    subtitle: 'Bandingkan revisi file biner, perbaiki checksum OEM, lalu simpan tanpa menimpa stok.',
    menuPath: 'Reverse Engineer -> Binary Diff Engine / Checksum & Integrity / File -> Save As...',
    figureCaption: 'Potongan tabel Binary Diff Engine bersama status verifikasi Checksum dan opsi Save As.',
    figureAriaLabel:
      'Ilustrasi potongan modal Binary Diff, kartu Verifikasi Checksum OEM, dan menu File Save As.',
    shortcuts: [
      { keys: ['Ctrl', 'B'], action: 'Buka Binary Diff Modal' },
      { keys: ['Ctrl', 'S'], action: 'Simpan ROM aktif' },
      { keys: ['Ctrl', 'Shift', 'S'], action: 'Save As salinan baru' },
    ],
    callouts: [
      {
        pin: 1,
        label: 'Binary Diff Modal',
        detail: 'Membandingkan ROM aktif dengan file pembanding serta mengekstrak area perubahan menjadi tabel baru.',
      },
      {
        pin: 2,
        label: 'Verify & Auto-Fix Checksum',
        detail: 'Menghitung ulang blok checksum sebelum file ditulis ulang ke ECU.',
      },
      {
        pin: 3,
        label: 'Save As Salinan Baru',
        detail: 'Menjaga file ROM stok asli tetap utuh sebagai cadangan pemulihan.',
      },
    ],
    steps: [
      'Buka Reverse Engineer -> Binary Diff Engine -> Open Diff Modal (Ctrl+B) untuk melihat daftar alamat ROM yang berbeda.',
      'Klik Extract jika ingin mengubah region hasil diff menjadi definisi peta kalibrasi baru.',
      'Periksa status Checksum di menu atas atau jalankan Reverse Engineer -> Checksum & Integrity -> Verify Checksum & Auto-Fix Checksum.',
      'Gunakan File -> Save As... (Ctrl+Shift+S) agar ROM hasil modifikasi tersimpan ke file terpisah dari ROM original.',
    ],
    tip: 'Tombol Revert Map to Stock di Inspector dapat mengembalikan nilai satu peta ke kondisi baseline ROM awal kapan pun dibutuhkan.',
    keywords: ['diff', 'binary diff', 'checksum', 'auto-fix', 'save', 'save as', 'revert', 'stock', 'oem', 'keihin'],
  },
  {
    id: 'telemetry-ecu',
    index: 7,
    shortTitle: 'Telemetry & ELM327',
    title: 'Live Telemetry RPM, ELM327 & Flash ECU',
    subtitle: 'Pantau RPM bergaya cockpit, baca/hapus kode DTC ELM327, serta read/write ECU ROM.',
    menuPath: 'Acquisition & ECU -> Live Telemetry & Auto-Tune / Flash ECU',
    figureCaption: 'Potongan panel Telemetry Dashboard (ARC/TRACK) bersama tab ECU Read & Flash dan Diagnostics ELM327.',
    figureAriaLabel:
      'Ilustrasi potongan kluster RPM Live Telemetry dan panel Read Flash serta Diagnostics ELM327 OBD.',
    shortcuts: [
      { keys: ['F9'], action: 'Mulai rekaman Datalog' },
      { keys: ['Esc'], action: 'Tutup jendela aktif teratas' },
    ],
    callouts: [
      {
        pin: 1,
        label: 'Supercar Cluster (ARC / TRACK)',
        detail: 'Menampilkan RPM, shift LED bar, gear, kecepatan, TPS, dan status AFR tanpa pergeseran kotak.',
      },
      {
        pin: 2,
        label: 'Diagnostics (ELM327 / OBD)',
        detail: 'Menjalankan Read DTCs, Clear DTCs, dan Reset ECU lengkap dengan konfirmasi keamanan.',
      },
      {
        pin: 3,
        label: 'Floating Multi-Window & Read/Flash',
        detail: 'Geser, ubah ukuran dari 8 sisi, sematkan ke atas (Pin), atau buka sebagai jendela desktop terpisah.',
      },
    ],
    steps: [
      'Buka Acquisition & ECU -> Live Telemetry & Auto-Tune -> Telemetry Dashboard (20Hz) untuk melihat meteran RPM dan metrik mesin.',
      'Gunakan mode ARC atau TRACK, serta muat sample/CSV datalog jika ingin menguji replay grafik sebelum terhubung ke kendaraan.',
      'Buka Acquisition & ECU -> Flash ECU -> Read Flash / Write Flash lalu pilih tab Diagnostics (ELM327 / OBD) untuk Read DTCs, Clear DTCs, atau Reset ECU.',
      'Untuk membaca dan menulis ROM ECU lewat adaptor K-Line/J2534, gunakan tab Read & Flash ROM dan pastikan tegangan aki serta preset ECU sesuai.',
    ],
    tip: 'Setiap popup penting (Telemetry, Flash, Engineering Toolkit, dan tutorial ini) bebas digeser dan diubah ukurannya hingga batas aman agar tidak saling menutupi.',
    keywords: ['telemetry', 'rpm', 'arc', 'track', 'elm327', 'obd', 'dtc', 'reset ecu', 'flash', 'read', 'write', 'window'],
  },
];

/** Filters tutorial chapters across title, subtitle, menu path, steps, tip, and keywords. */
export function filterTutorialChapters(
  chapters: readonly TutorialChapter[],
  query: string
): TutorialChapter[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...chapters];

  return chapters.filter((ch) => {
    const haystack = [
      ch.shortTitle,
      ch.title,
      ch.subtitle,
      ch.menuPath,
      ch.tip,
      ...ch.keywords,
      ...ch.steps,
      ...ch.callouts.map((c) => `${c.label} ${c.detail}`),
    ]
      .join(' ')
      .toLowerCase();
    return haystack.includes(q);
  });
}

/** Resolves previous or next chapter ID within the provided ordered list. */
export function resolveAdjacentChapterId(
  chapters: readonly TutorialChapter[],
  currentId: TutorialSceneId,
  delta: -1 | 1
): TutorialSceneId | null {
  const idx = chapters.findIndex((c) => c.id === currentId);
  if (idx < 0) return null;
  const next = chapters[idx + delta];
  return next ? next.id : null;
}
