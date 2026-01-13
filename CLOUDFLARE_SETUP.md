# Cloudflare Pages Setup Guide

Complete guide for deploying Walsync documentation to Cloudflare Pages with custom domain.

## ✅ What You Have

- ✅ Domain: `walsync.dev` (purchased)
- ✅ Astro Starlight documentation site
- ✅ GitHub repository
- ✅ GitHub Actions workflow

## Step 1: Add Domain to Cloudflare

### 1a. Add Domain to Cloudflare Account

1. Go to [Cloudflare Dashboard](https://dash.cloudflare.com/)
2. Click **"Add a site"**
3. Enter domain: `walsync.dev`
4. Select **Free** plan
5. Click **"Continue setup"**

### 1b. Update Nameservers at Domain Registrar

Cloudflare will provide 2 nameservers:
- Example: `grace.ns.cloudflare.com`
- Example: `logan.ns.cloudflare.com`

Go to your domain registrar (wherever you bought walsync.dev):
1. Find DNS/Nameserver settings
2. Replace with Cloudflare nameservers
3. Wait for propagation (can take 24-48 hours)

Verify with:
```bash
nslookup walsync.dev
# Should show Cloudflare nameservers
```

## Step 2: Create Cloudflare Pages Project

### 2a. Connect GitHub Repository

1. Go to [Cloudflare Dashboard](https://dash.cloudflare.com/)
2. Navigate to **Pages**
3. Click **"Create a project"**
4. Select **"Connect to Git"**
5. Authenticate with GitHub
6. Select repository: `personal-website`

### 2b. Configure Build Settings

1. **Production branch**: `main`
2. **Framework preset**: `Astro`
3. **Build command**: `cd docs && npm run build`
4. **Build output directory**: `docs/dist`

Environment variables (optional):
- No additional env vars needed

### 2c. Create Project

Click **"Save and Deploy"**

Cloudflare will:
- ✓ Clone your repository
- ✓ Run build command
- ✓ Deploy `dist/` folder
- ✓ Assign temporary URL

## Step 3: Connect Custom Domain

### 3a. Add Custom Domain

1. In Cloudflare Pages project settings
2. Navigate to **"Custom domains"**
3. Click **"Set up custom domain"**
4. Enter: `walsync.dev`
5. Click **"Continue"**

### 3b. Verify DNS

Cloudflare shows DNS records to add:

```
Type: CNAME
Name: walsync.dev (or @)
Content: <project-name>.pages.dev
TTL: Auto
```

This is typically **automatic** if nameservers are set correctly.

### 3c. Verify Setup

```bash
# Check DNS is set correctly
dig walsync.dev CNAME

# Check SSL certificate (takes a few minutes)
curl -I https://walsync.dev
# Should show: HTTP/2 200
```

## Step 4: GitHub Setup

### 4a. Create GitHub Secrets

Add these secrets to your repository:

**Settings → Secrets and variables → Actions → New repository secret**

1. **CLOUDFLARE_API_TOKEN**
   - Go to [Cloudflare API Tokens](https://dash.cloudflare.com/profile/api-tokens)
   - Create token with permissions:
     - Cloudflare Pages (Read & Edit)
     - Zone (Read)
   - Copy token and paste as secret

2. **CLOUDFLARE_ACCOUNT_ID**
   - Dashboard → bottom right, copy Account ID
   - Paste as secret

### 4b. Verify Secrets

```bash
# Check that secrets are set (GitHub doesn't show values, just names)
# View at: Settings → Secrets and variables → Actions
```

## Step 5: Test Deployment

### 5a. Trigger Deployment

Push to main branch:

```bash
git add .
git commit -m "test: trigger cloudflare deployment"
git push origin main
```

### 5b. Monitor Build

1. Go to repository **Actions** tab
2. Watch "Deploy to Cloudflare Pages" workflow
3. Should show:
   - ✓ Node setup
   - ✓ Dependencies installed
   - ✓ Documentation built
   - ✓ Deployed to Cloudflare

### 5c. Verify Live

```bash
# Check site is live
curl -I https://walsync.dev
# HTTP/2 200 OK

# Visit in browser
https://walsync.dev
```

## Step 6: SSL/TLS Configuration (Optional)

Cloudflare automatically provides SSL certificates.

To verify:
1. Dashboard → Sites → walsync.dev
2. **SSL/TLS** → **Overview**
3. Should show: **Full (strict)** or **Full**

## Automatic Deployments

After setup, every push to `main` triggers automatic deployment:

```
Push to main
    ↓
GitHub Actions starts
    ↓
Builds documentation (npm run build)
    ↓
Deploys to Cloudflare Pages
    ↓
Site updated at https://walsync.dev
```

No manual intervention needed!

## Troubleshooting

### DNS Not Resolving

```bash
# Check nameservers
nslookup -type=NS walsync.dev

# Should show Cloudflare nameservers
# If not, wait 24-48 hours for propagation
```

### Build Failing

Check GitHub Actions logs:
1. Repository → **Actions** tab
2. Click failed workflow
3. Expand job logs
4. Common issues:
   - Node version mismatch (should be 18+)
   - npm install failing (check dependencies)
   - Build path incorrect (should be `docs/dist`)

### SSL Certificate Error

```bash
# Give Cloudflare time to issue certificate
# Usually takes 5-15 minutes after DNS verification

# Check certificate
curl -v https://walsync.dev 2>&1 | grep "certificate"
```

### Pages Not Updating

1. Check build completed successfully
2. Clear browser cache (Cmd+Shift+R or Ctrl+Shift+R)
3. Check Cloudflare cache settings:
   - Dashboard → **Caching** → **Cache Everything** rule (optional)

## Advanced Configuration

### Environment Variables

If needed for build:

```bash
# In Cloudflare Pages project settings
# Environment variables → Production

# Example (not needed for Walsync):
# SITE_URL=https://walsync.dev
```

### Redirects

Edit `docs/_redirects` file:

```
# Example redirects (if needed)
/docs/* /  200
```

### Custom 404 Page

Cloudflare uses static file from `dist/404.html` (Astro generates this automatically).

## Monitoring

### Build Logs

- Repository **Actions** tab → Workflow runs
- Each push shows build details
- Successful deploys show green checkmark

### Site Analytics

Cloudflare provides:
- **Analytics** → Performance metrics
- Page views, request counts
- Error rates (4xx, 5xx)

## DNS Records (Reference)

After setup, your DNS should look like:

```
Type    Name        Content                  TTL
CNAME   walsync.dev <project>.pages.dev      Auto
```

No other records needed!

## Rollback

If something breaks:

1. Go to Cloudflare Pages project
2. **Deployments** tab
3. Find previous working deployment
4. Click **"Rollback"**

Or revert GitHub commit and push new commit.

## Custom Headers (Optional)

To add security headers, create `docs/_headers`:

```
/*
  X-Frame-Options: SAMEORIGIN
  X-Content-Type-Options: nosniff
  Referrer-Policy: strict-origin-when-cross-origin
```

## Useful Links

- [Cloudflare Dashboard](https://dash.cloudflare.com/)
- [Pages Documentation](https://developers.cloudflare.com/pages/)
- [Astro Cloudflare Integration](https://docs.astro.build/guides/integrations/cloudflare/)
- [DNS Checker](https://dnschecker.org/)

## Summary

✅ Setup complete when:
- [ ] Domain added to Cloudflare
- [ ] Nameservers updated at registrar
- [ ] Pages project created
- [ ] Custom domain connected
- [ ] GitHub secrets configured
- [ ] First deployment successful
- [ ] Site live at https://walsync.dev
- [ ] SSL certificate active

---

**All set!** Your Walsync documentation is now live on a professional domain! 🎉

Every push to `main` automatically deploys the latest docs to Cloudflare Pages.
