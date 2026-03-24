# Design Reference — from legacy rocket-api/frontend

## Theme: HSL-based blue primary, class-based dark mode

### Light Mode CSS Variables
```
--background: 214 44% 98%
--foreground: 222 47% 11%
--card: 0 0% 100%
--card-foreground: 222 47% 11%
--popover: 0 0% 100%
--popover-foreground: 222 47% 11%
--primary: 213 88% 42%
--primary-foreground: 0 0% 100%
--secondary: 210 40% 95%
--secondary-foreground: 222 47% 11%
--muted: 213 35% 94%
--muted-foreground: 215 17% 40%
--accent: 212 86% 94%
--accent-foreground: 213 88% 30%
--destructive: 0 84.2% 60.2%
--destructive-foreground: 0 0% 98%
--border: 214 31% 88%
--input: 214 31% 88%
--ring: 213 88% 42%
--radius: 0.7rem
```

### Dark Mode CSS Variables
```
--background: 0 0% 12%
--foreground: 0 0% 87%
--card: 0 0% 10%
--card-foreground: 0 0% 87%
--popover: 0 0% 10%
--popover-foreground: 0 0% 87%
--primary: 207 100% 42%
--primary-foreground: 0 0% 98%
--secondary: 0 0% 15%
--secondary-foreground: 0 0% 87%
--muted: 0 0% 14%
--muted-foreground: 0 0% 60%
--accent: 204 100% 18%
--accent-foreground: 0 0% 90%
--destructive: 0 72% 45%
--destructive-foreground: 0 0% 98%
--border: 0 0% 19%
--input: 0 0% 19%
--ring: 207 100% 42%
```

### HTTP Method Colors
- GET: text-green-600 / bg-green-500
- POST: text-blue-600 / bg-blue-500
- PUT: text-orange-600 / bg-orange-500
- PATCH: text-yellow-600 / bg-yellow-500
- DELETE: text-red-600 / bg-red-500
- HEAD/OPTIONS: text-gray-500 / bg-gray-400

### Response Status Colors
- 2xx: bg-green-100 text-green-700
- 3xx: bg-yellow-100 text-yellow-700
- 4xx: bg-red-100 text-red-700
- 5xx: bg-red-100 text-red-700

### Font: Inter Variable (not JetBrains Mono)
### Radius: 0.7rem
### Border opacity: 70% pattern (border-border/70)
### Glass effects: bg-card/70 backdrop-blur-sm
### Gradient: bg-gradient-to-br from-background via-background to-accent/25
