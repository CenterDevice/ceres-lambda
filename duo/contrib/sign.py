#!/usr/bin/env python2

import base64, hmac, hashlib, urllib

def sign(method, host, path, params, skey, ikey):
    """
    Return HTTP Basic Authentication ("Authorization" and "Date") headers.
    method, host, path: strings from request
    params: dict of request parameters
    skey: secret key
    ikey: integration key
    """

    # create canonical string
    now = "Tue, 21 Aug 2012 17:29:18 -0000"
    canon = [now, method.upper(), host.lower(), path]
    args = []
    for key in sorted(params.keys()):
        val = params[key]
        if isinstance(val, unicode):
            val = val.encode("utf-8")
        args.append(
            '%s=%s' % (urllib.quote(key, '~'), urllib.quote(val, '~')))
    canon.append('&'.join(args))
    canon = '\n'.join(canon)
    print("'%s'" % canon)

    # sign canonical string
    sig = hmac.new(skey, canon, hashlib.sha1)
    auth = '%s:%s' % (ikey, sig.hexdigest())
    print(auth)

    # return headers
    return {'Date': now, 'Authorization': 'Basic %s' % base64.b64encode(auth)}

if __name__ == '__main__':

    params = dict()
    # params["realname"] = u"First Last"
    # params["username"] = u"root"
    params["state"] = u"disabled"

    result = sign("POST", "api-XXXXXXXX.duosecurity.com", "/admin/v1/users", params, "Zh5eGmUq9zpfQnyUIu5OL9iWoMMv5ZNmk3zLJ4Ep", "DIWJ8X6AEYOR5OMC6TQ1")

    print(result)
